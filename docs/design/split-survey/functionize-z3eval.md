# Split survey — functionize / z3_eval

## Summary

- 8 files surveyed: `z3_eval.rs` (1260 LOC), `functionize/cranelift.rs` (1388),
  `functionize/symbolic.rs` (911), `functionize/satisfier.rs` (235),
  `functionize/glsl.rs` (645), `functionize/llm.rs` (643), `functionize/mod.rs` (20),
  `core/functionizer.rs` (28). Plus adjacents noted: `decompose.rs` (257),
  `z3_profile.rs` (159).
- Class breakdown: **engine** (8), **front-end** (0), **entangled** (0).

### Headline findings

**(a) Does the functionizer survive the split untouched?**
Yes — with one significant asterisk. Every functionizer (`cranelift.rs`,
`symbolic.rs`, `satisfier.rs`, `glsl.rs`, `llm.rs`) receives a `&Z3Program`
(from `core/z3_program.rs`) and `&EnumRegistry`/`&DatatypeRegistry`. None import
anything from `translate/`, `parser.rs`, or the Evident AST beyond the
`has_known_translator_gap` gate in `z3_eval.rs` (which touches `core::ast`). The
functionizer layer is source-agnostic at its own call site. **The asterisk is
`z3_eval.rs` itself** (see (b)).

**(b) Is `z3_eval.rs` welded to the in-memory C-API AST?**
Yes, structurally. `z3_eval.rs` receives `&[Bool<'ctx>]` — live `z3::ast` handle
slices. It operates on `Dynamic<'ctx>` nodes by walking `AstKind`, `DeclKind`,
`num_children()`, and `children()` — the Z3 C-API term-inspection API. It does
**not** parse text; it pattern-matches the in-memory AST representation produced
by `translate/`. However, the Z3 C-API is AST-source-agnostic: the same
`Bool<'ctx>` handles can be obtained by parsing SMT-LIB text via
`z3::Context::from_string` / a Z3 `Solver`'s `from_smtlib2_string` family.
The weld is to the *handle type*, not to the *translate/ pipeline*. If the engine
re-ingests SMT-LIB and obtains live handles into the same `'static Context`, the
`simplify_assertions` → `extract_program_partial` path runs unchanged.

**(c) Is `Z3Program` a serializable value-IR and candidate alternative seam?**
**No.** `core/z3_program.rs:10-13` makes this unambiguous: `Z3Program<'ctx>`
carries `Dynamic<'ctx>` in every step variant (`Scalar`, `Seq`, `Guarded`) and in
`checks` and `predicates`. The `'ctx` lifetime is the Z3 `Context` lifetime.
`Z3Program` cannot be serialized, deserialized, or passed across a process
boundary. The only seam candidates are either the SMT-LIB text itself
(text-serializable, front-end output) or the `Value`-only outputs of compiled
functions (already clean). `Z3Program` is an **internal optimizer IR**, not a
cross-seam wire format.

---

## Per-file classification

| File | LOC | Class | Why | Seam difficulty | Cross-seam coupling |
|---|---|---|---|---|---|
| `z3_eval.rs` | 1260 | engine | Consumes `Bool<'ctx>` / `Dynamic<'ctx>` live Z3 handles; produces `Z3Program<'ctx>` with embedded handles; Z3 tactic application (`simplify`, `propagate-values`); lives entirely in the solve path | low | Accepts `&[Bool<'ctx>]` — same type whether handles came from `translate/` or from parsing SMT-LIB into the same `Context`. No `translate/` imports. One `core::ast` reference in `has_known_translator_gap` (a front-end guard on the AST, see below). |
| `functionize/cranelift.rs` | 1388 | engine | Pure `Z3Program<'ctx>` → Cranelift IR → native code; imports only `crate::z3_eval::{Z3Program, Z3Step, GuardedBody}` and `crate::translate::{EnumRegistry, Value}` (both are shared core types) | low | No `translate/` logic; no Evident AST; walks `Dynamic<'ctx>` nodes via `AstKind`/`DeclKind` to classify operands (Int/Bool/String/enum) — same weld as `z3_eval.rs`, same resolution. |
| `functionize/symbolic.rs` | 911 | engine | GP search over Z3-sampled I/O pairs; takes `&Z3Program`, calls `Solver` to draw samples from the live handles | low | Imports `Z3Program`, `Z3Step`; creates a fresh `Solver` and binds inputs from `Dynamic` exprs (`step_exprs`). Needs live handles only for sampling — can be re-derived from either source. |
| `functionize/satisfier.rs` | 235 | engine | Strips `Sample*` steps, seeds a PRNG, delegates rest to `CraneliftFunctionizer`; pure `Z3Program` → `CompiledFunction` | low | Only `Z3Program`, `Z3Step`, `EnumRegistry`, `Value`. No Z3 C-API calls; no `Dynamic` inspection. |
| `functionize/glsl.rs` | 645 | engine (macOS-only) | Transpiles scalar `Z3Program` to GLSL; runs GL draw pass; reads back results | low | Imports `Z3Program`, `Z3Step`, `Dynamic`; walks `AstKind`/`DeclKind` to emit GLSL operators. Same Z3 AST weld as cranelift. |
| `functionize/llm.rs` | 643 | engine | LLM-generates Rust source, compiles via `rustc`, validates against Z3 samples | low | `Z3Program`, `Z3Step`, `Dynamic` (for sampling only); `AnthropicGenerator` HTTP call; same C-API handle weld for sampling. |
| `functionize/mod.rs` | 20 | engine | Module declarations + `default()` factory; re-exports `Functionizer`/`CompiledFunction` traits | none | No coupling beyond module-level re-exports. |
| `core/functionizer.rs` | 28 | engine (shared types) | Trait definitions only: `Functionizer::compile(&Z3Program, …)` + `CompiledFunction::call` | none | Clean interface boundary. `Z3Program` is the only non-trivial type at this surface; see seam note on its non-serializability. |

Adjacent files (not deep-dived):
- `decompose.rs` (257 LOC): takes `&[Bool<'ctx>]`, runs Z3 `simplify` tactic, union-finds components — engine, same Z3-handle weld as `z3_eval.rs`.
- `z3_profile.rs` (159 LOC): solver statistics aggregation; wraps `z3::Solver` — pure engine bookkeeping, no seam concern.

---

## Seam notes

### The functionizer is source-agnostic — but only below `z3_eval.rs`

The north-star claim "functionizers consume Z3 AST and don't care about its
source" is **true for `functionize/` but not yet true for the full pipeline**. The
`Functionizer::compile` trait (`core/functionizer.rs:16`) takes `&Z3Program` —
a type that embeds `Dynamic<'ctx>` at every step. The steps in `Z3Program` are
produced by `z3_eval::extract_program_partial` (`z3_eval.rs:182`), which is
called from `runtime/query.rs:666` after `simplify_assertions` (`query.rs:489`).

The `simplify_assertions` call at `query.rs:489` receives:
```rust
let assertions_local = cached.solver.get_assertions();  // query.rs:484
```
`cached` is a `CachedSchema` built by `translate::build_cache` (`query.rs:479`).
The chain is: `translate/` → `CachedSchema::solver` → `.get_assertions()` →
`simplify_assertions` → `extract_program_partial` → `Z3Program` → `compile`.

The key observation is that **`z3_eval.rs` only cares about the shape of the
`Bool<'ctx>` AST nodes, not how they were created**. It calls
`a.kind()` (`AstKind::App`, `Numeral`, etc.), `a.safe_decl()`, `a.children()`,
`a.as_bool()`, `a.as_int()` — all structural term-inspection methods that work
identically whether the handles came from `translate/`'s constraint-building API
or from Z3 parsing SMT-LIB text (`Solver::from_smtlib2_string` / `Context::from_string`).

So the current weld is: **the engine requires a live `z3::Context` with asserted
formulas**. After the split, if the engine re-ingests the SMT-LIB text the
front-end emitted (via `Context::from_string` or by asserting the parsed
formulas), the entire `z3_eval.rs` + `functionize/` chain runs unchanged. The
functionizer does NOT need to be modified; `z3_eval.rs` does NOT need to be
modified. The wire crossing point is the SMT-LIB text, not `Z3Program`.

### `z3_eval.rs:has_known_translator_gap` — the one AST-touching exception

`z3_eval.rs:1195` imports and walks `core::ast::BodyItem` / `core::ast::Expr`:
```rust
pub fn has_known_translator_gap(body: &[crate::core::ast::BodyItem]) -> bool {
```
This is called in `runtime/query.rs:471` as a **load-time guard** before the
`translate/` path runs. It is a front-end concern (pre-translation AST check)
sitting in an otherwise engine-side file. After the split it should move to the
front-end or to a utility invoked only during front-end compilation. It does not
affect the JIT pipeline itself; it gates entry to the pipeline.

### `Z3Program` is not a cross-seam IR

`core/z3_program.rs:4` imports `z3::ast::{Bool, Dynamic}`. Every variant of
`Z3Step<'ctx>` that carries computation (`Scalar`, `Seq`, `Guarded`) embeds
`Dynamic<'ctx>`:
```rust
Z3Step::Scalar { var: String, expr: Dynamic<'ctx> },
Z3Step::Seq    { var: String, elem_exprs: Vec<Dynamic<'ctx>> },
Z3Step::Guarded { var: String, branches: Vec<GuardedBranch<'ctx>> },
```
`GuardedBranch` and `GuardedBody` both hold `Dynamic<'ctx>`. The `checks` and
`predicates` fields of `Z3Program` are `Vec<(Dynamic<'ctx>, Dynamic<'ctx>)>` and
`Vec<Bool<'ctx>>` respectively (`z3_program.rs:14-16`). The `'ctx` parameter
binds the struct to a live `z3::Context`; there is no `serde`, no `Display`
serialization round-trip, no way to reconstruct from text. `Z3Program` is not a
candidate cross-seam wire format.

The three `Sample*` variants (`SampleRange`, `SampleEnum`, `SampleSet`) are the
only steps that carry no `Dynamic<'ctx>` and could in principle be serialized —
but they are a minor path (satisfier only) and not sufficient to carry the full
program.

### Cranelift and GLSL walk `Dynamic<'ctx>` directly

`cranelift.rs:436` iterates `program.steps`, dispatches on `Z3Step`, and for
`Scalar` steps calls `emit_write_value(..., expr, ...)` where `expr` is a
`&Dynamic<'ctx>`. The emitter (`emit_compute_i64`, `emit_write_value`) recursively
walks the Z3 AST node via `d.kind()`, `d.safe_decl().kind()`, `d.children()` to
classify arithmetic operators and generate Cranelift IR. The `glsl.rs` transpiler
does the same. This is the source-agnostic property in action: they walk whatever
Z3 AST happens to be in the node, regardless of origin.

### Invocation site straddles front-end setup and engine tick

`compile_one_component` (`query.rs:644`) is called during the **first-tick cache
miss** — it is the boundary between load/setup and per-tick execution. After this
call, the `ClaimPlan` is cached and the per-tick hot path only calls
`execute_plan`. The functionizer `compile` call is therefore **setup-phase only**
(confirmed by session ZZ / the AOT-over-runtime priority). After the split,
`compile_one_component` belongs to the engine's setup phase, taking SMT-LIB text
as input (reasserting it into a fresh context, then running `simplify_assertions`
→ `extract_program_partial` → `compile`).

### What would actually need to change post-split

Nothing in `functionize/` itself. The required changes are upstream:
1. The engine's setup phase must re-ingest SMT-LIB text into a `z3::Context`
   (replacing the current `translate::build_cache` call).
2. `has_known_translator_gap` (currently in `z3_eval.rs`) must move to the
   front-end, since it gates a front-end AST check.
3. The `'static` lifetime transmute at `query.rs:485-488` (which ties the engine
   to the specific `'static Context` instance) becomes the engine's own Context
   lifetime concern — no change to `z3_eval.rs` logic, only to where the Context
   lives.
