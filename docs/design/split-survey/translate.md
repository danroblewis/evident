# Split survey — translate

## Summary

35 files (including `translate.rs` entry point); 13,131 total LOC.

**Class counts:**
- Front-end: 12 files (~2,650 LOC) — pure AST→semantics, no solver.
- Engine: 11 files (~2,050 LOC) — drive/decode the Z3 solver; irreducibly engine-side.
- Entangled: 12 files (~8,430 LOC) — straddle both halves; the vast majority by LOC.

**Headline findings:**

(a) **"Emit text once" vs. "interleaved with live solving"**: The ONLY file that
already emits SMT-LIB text is `smtlib.rs` (495 LOC), and it covers a narrow
scalar-only subset. The remaining ~12,600 LOC of translate/ build live Z3 AST
handles via the C API. More critically, the `inline/` cluster (walk, calls,
membership, guards) **interleaves solver queries with translation**: `guard_is_satisfiable`
(`inline/guards.rs:17-35`) calls `solver.push()`, `solver.assert(g)`, `solver.check()`,
and `solver.pop(1)` in the middle of inlining every guarded claim invocation. This is
the single deepest blocker for a clean "emit text first, solve second" split.

(b) **Biggest blockers to a full-language SMT-LIB text emitter**: (1) The solver-mid-
translation entanglement in `inline/guards.rs` (pruning dead branches with `solver.check()`
during inlining); (2) the entire `declare.rs`/`datatypes.rs` layer builds live Z3 `Sort`,
`Array`, `DatatypeSort`, and `Int` handles at declaration time — these handles are stored in
`Var<'ctx>` and threaded through every `translate_*` call; (3) `exprs/` translators return
`z3::ast::Bool<'ctx>` / `Int<'ctx>` / etc.  (live handle types) so every expr translator
must be rewritten to emit text; (4) `exprs/string_ops.rs` calls raw `z3_sys` C API
functions directly (no text equivalent exists in the z3 crate); (5) the enum/datatype
machinery (`datatypes.rs`, `declare.rs`, `exprs/enums.rs`) builds `DatatypeSort` at load
time via `DatatypeBuilder` — porting to `(declare-datatype ...)` SMT-LIB text is doable
but non-trivial. Collectively, rewriting `inline/` + `exprs/` + `declare/` + `datatypes`
to emit text instead of live handles is a ~7,000-LOC rewrite covering essentially the
entire constraint-translation core.

(c) **Irreducibly engine-side**: `eval/` cluster (mod.rs + solver.rs + cached.rs + decode.rs
+ core.rs + decompose.rs + extra.rs — ~1,450 LOC). These run the Z3 solver, extract models,
run UNSAT-core queries, classify components with 2-copy satisfiability checks, and manage
push/pop state. `extract.rs` (518 LOC) is also engine-side: it reads sequences, composites,
sets and enums out of Z3 models. `encode_ast.rs` (964 LOC) is entangled — it encodes Rust
ASTs as Z3 Datatype values so self-hosted passes can receive them; `decode_ast.rs` (934 LOC)
decodes Z3/Value outputs back to Rust AST — both are needed when crossing the self-hosting
seam but are otherwise load-side rather than per-tick.

---

## Per-file classification

| File | LOC | Class | Why | Seam difficulty | Text-emit gap | Cross-seam coupling |
|---|---|---|---|---|---|---|
| `translate.rs` | 63 | front-end | Re-export module; no Z3 C-API calls | low | none (no translation logic) | Re-exports both halves; splits cleanly |
| `preprocess.rs` | 370 | front-end | Pure AST + Value constant-folding; no Z3 handles | low | none (no text to emit; already context-free) | `apply_pinned_ints` takes `Var<'ctx>` — single entanglement point at output boundary |
| `smtlib.rs` | 495 | front-end (partial) | Emits SMT-LIB TEXT for scalar subset; also calls `Solver::from_string` / `solver.check()` | low (text half) / med (solve half) | none for the emit half; the `solve()` function is engine-side | `z3::Solver` + `z3::Context` used in `solve()` only; `emit()` / `schema_to_smtlib()` are purely text |
| `datatypes.rs` | 147 | entangled | Builds live `DatatypeSort<'static>` (C-API) from user type schemas; returns `&'static DatatypeSort<'static>` handles | high | fundamental rewrite — must emit `(declare-datatype ...)` SMT-LIB instead of building in-memory sort | `DatatypeBuilder::new`, `DatatypeSort`, `Sort::array`, all Z3 live handles |
| `declare.rs` | 266 | entangled | Builds live `Int`, `Bool`, `Array`, `Set`, `Datatype` Z3 consts for each typed variable; stores in `Var<'ctx>` | high | fundamental rewrite — must emit `(declare-const ...)` text; `Var<'ctx>` type itself must change to a name-string | `z3::ast::*`, `Context`, `Sort`; all live-handle APIs |
| `extract.rs` | 518 | engine | Reads Seqs/Sets/Composites/Enums out of Z3 model; pins given values as Z3 assertions | low | N/A (engine-only; model decode has no text equivalent) | `z3::Model`, `Array`, `Int`, `Bool`, `Ast`, `DatatypeSort` |
| `encode_ast.rs` | 964 | entangled | Encodes Rust `Program` AST as Z3 `Datatype` for self-hosted pass injection | high | Cannot become text-emit: purpose is to pin a live Z3 Datatype into a solver assertion; text encoding (`(assert (= prog_var ...))`) requires redesign of the self-hosting seam | `z3::ast::Datatype`, `DatatypeSort`, `Context`, `EnumRegistry` |
| `decode_ast.rs` | 934 | front-end | Decodes `Value::Enum` back to Rust AST; operates on `Value` (Rust struct), not live Z3 handles | low | none (already Z3-context-free) | No Z3 live handles; reads `Value::Enum` produced by eval decoding |
| `exprs/mod.rs` | 79 | entangled | Thread-local `EnumRegistry` + `DatatypeSort` guard for translation context | high | Guard installs `*const EnumRegistry` needed to resolve enum constructors — requires reconceptualizing if context is a text emitter | `DatatypeSort<'static>` in TARGET_ENUM_HINT; raw pointer to `EnumRegistry` |
| `exprs/scalar.rs` | 256 | entangled | Translates scalar Exprs to live Z3 `Int<'ctx>` / `Real<'ctx>` / `Z3Str<'ctx>` | high | Fundamental rewrite to emit s-expression strings; return types are live handles | All Z3 ast types; `translate_bool` cross-call |
| `exprs/bool.rs` | 415 | entangled | Translates Bool-shape Exprs to `Bool<'ctx>`; dispatches to scalar/seq/enum/record translators | high | Entire function signature returns `Bool<'ctx>`; all branches produce live handles | All Z3 ast types; Schema dispatch via `schemas` map |
| `exprs/enums.rs` | 253 | entangled | Resolves enum Exprs to `z3::ast::Datatype<'ctx>`; builds Cons-chains | high | Must emit constructor application in text (`(mk_Effect ...)` etc.) | `DatatypeSort`, `Datatype`, `z3::ast::Ast` |
| `exprs/quant.rs` | 250 | entangled | Unrolls ∀/∃ over Seqs/ranges to Z3 `Bool::and`/`Bool::or` | high | Unrolling logic is sound, but result type is live `Bool<'ctx>`; rewrite to emit `(and ...)` text | `Bool<'ctx>`, `Int`, `Var<'ctx>` |
| `exprs/seq_eq.rs` | 485 | entangled | Translates Seq/Set equality, composite element assignment, binding helpers | high | Complex; Array select/store patterns, Cons chains — each has an SMT-LIB text form but requires full rewrite | `Array`, `Bool`, `Int`, `DatatypeSort` |
| `exprs/seq_field.rs` | 165 | entangled | Resolves Seq field handles (dotted field paths through composite Seqs) | high | Returns `SeqHandleRef` carrying live Z3 handles | `Array`, `Int`, `DatatypeSort`, `Var<'ctx>` |
| `exprs/record_lift.rs` | 290 | entangled | Lifts record-typed arithmetic/comparisons componentwise to scalar constraints | high | Logic is correct; output is `Bool<'ctx>`; needs rewrite to text | `Bool<'ctx>`, `Int`, `translate_bool` |
| `exprs/match_expr.rs` | 183 | entangled | Translates match-arm scrutinee to ITE chain; `fold_arms_to_ite` | high | ITE chain has SMT-LIB form `(ite ...)` but return type is live handle | `Bool<'ctx>`, `Int<'ctx>`, `Datatype` |
| `exprs/range.rs` | 28 | front-end | Pure: `literal_range` returns `Option<(i64, i64)>` from Range AST | low | none | No Z3 handles |
| `exprs/string_ops.rs` | 193 | entangled | String builtins via raw `z3_sys` C API (`Z3_mk_seq_length`, `Z3_mk_seq_extract`, etc.) | high | Fundamental blocker: uses raw `z3_sys` because the z3 crate wrapper doesn't expose these; text form would be `(str.len ...)` etc. but requires replacing the C-API calls | `z3_sys::*`, `Context`, raw Z3 C API |
| `exprs/mapping.rs` | 236 | entangled | Resolves claim-slot mappings to `Var<'ctx>` entries in env | high | Returns `HashMap<String, Var<'ctx>>` — env entries are live handles | `Var<'ctx>`, `translate_bool`, `translate_int`, `translate_str` |
| `inline/mod.rs` | 13 | entangled | Re-exports inline entry points | trivial | — | Trivially follows inlining module |
| `inline/walk.rs` | 209 | entangled | Main body-item dispatch loop; calls `translate_bool`, assertion, passthrough recursion | high | Critical coupling: calls `guard_is_satisfiable` which runs `solver.check()` mid-translation (see Seam notes) | `Solver<'static>`, `Context`, `Bool`, `DatatypeRegistry` |
| `inline/calls.rs` | 309 | entangled | Claim/tuple/guarded invocation inlining; clones env, isolates locals, recurses | high | All inlining materializes Z3 consts via `declare_var_named` and asserts via `track_assert` | `Solver<'static>`, `Var<'ctx>`, `declare_var_named` |
| `inline/membership.rs` | 179 | entangled | Membership body-item handler; inherits type-body constraints per instance and per Seq element | high | Queries `var.as_datatype_seq()` / `var.as_seq()` for live len values mid-inlining | `Solver<'static>`, `Bool`, `Var<'ctx>`, `translate_bool` |
| `inline/dispatch.rs` | 204 | front-end | AST-level name resolution: `CallDispatch` enum, `resolve_call`, `resolve_forall_unroll` | med | Logic is sound; uses `env.get(..).as_seq()` to read pinned lengths from live `Var<'ctx>` — the ONLY Z3 coupling | `Var<'ctx>` read-only: `var.as_seq()` / `var.as_datatype_seq()` — to emit text, lengths must come from a precomputed metadata map instead |
| `inline/guards.rs` | 56 | engine | `guard_is_satisfiable` runs `solver.push/assert/check/pop`; `guarded_bool` / `compose_guards` build live `Bool<'ctx>` | high | Irreducibly engine-side: satisfiability check during translation is the core entanglement | `Solver<'static>`, `Bool<'ctx>`, Z3 `SatResult` |
| `inline/recursion.rs` | 59 | front-end | Depth counter for inlining recursion; `isolate_helper_locals` strips locals from env clone | low | None: AST + `HashMap<String,Var<'ctx>>` clone only; no Z3 calls | Clones `Var<'ctx>` entries but makes no Z3 API calls |
| `inline/rewrite.rs` | 155 | front-end | Pure AST→AST rewriters: `rewrite_idents_with_prefix`, `substitute_bound_var` | low | none | No Z3 handles — purely structural AST manipulation |
| `inline/subschema.rs` | 179 | entangled | Subclaim-of-type + ∀-over-subclaim inlining; receiver field rebinding | high | Calls `guard_is_satisfiable` (engine), `translate_bool` (entangled), `declare_var_named` (entangled) | `Solver<'static>`, `Bool`, `Context`, `DatatypeRegistry` |
| `eval/mod.rs` | 189 | engine | Canonical `evaluate`: declare, pin, inline, check, extract model | low | N/A | `Solver`, `Context`, `Model`, `SatResult` |
| `eval/solver.rs` | 122 | engine | Tactic chain + solver tuning; `declare_and_assert`; `populate_enum_variants`; f64↔Z3-Real | low | N/A | `Solver`, `Params`, `Tactic`, `Context` |
| `eval/cached.rs` | 298 | engine | `build_cache` / `run_cached` / `sample_cached_inner`: push/assert-givens/check/pop per tick | low | N/A | `CachedSchema`, `Solver`, `Model`, push/pop |
| `eval/decode.rs` | 279 | engine | `extract_binding`, `extract_enum_value`, `extract_seq_enum`, Cons-chain walk | low | N/A | `z3::Model`, `Datatype`, `Array`, `Int` |
| `eval/core.rs` | 110 | engine | UNSAT-core variant: `evaluate_with_core`, tracker Bool injection, `get_unsat_core` | low | N/A | `Solver::check_assumptions`, `get_unsat_core` |
| `eval/extra.rs` | 271 | engine | Extra-assertion variants for self-hosted pass injection + body-seq pinning | low | N/A | `Solver`, `Datatype`, `encode_ast` |
| `eval/decompose.rs` | 278 | engine | Structural decomposition + 2-copy functional check; calls `solver.check()` per component | low | N/A | `Solver`, `Model`, push/pop per component |

---

## Seam notes

### The in-memory Z3 AST vs. SMT-LIB text coupling

The existing translate/ stack is built entirely on live Z3 handle passing. The central data
structure is `Var<'ctx>`, which carries live Z3 typed AST nodes:

- `Var::IntVar(Int<'ctx>)`, `Var::BoolVar(Bool<'ctx>)`, `Var::SeqVar { arr: Array<'ctx>, len: Int<'ctx>, … }`, `Var::DatatypeSeqVar { arr, len, dt: &'static DatatypeSort<'static>, … }`, etc.

This `Var<'ctx>` map (the `env`) is threaded through every translation function. The
`translate_int` / `translate_bool` / `translate_str` / `resolve_enum_ast` functions in
`exprs/` return live Z3 handle types (`Int<'ctx>`, `Bool<'ctx>`, `Z3Str<'ctx>`,
`z3::ast::Datatype<'ctx>`). Changing these to emit text instead requires replacing ALL
return types and ALL function signatures in the entire `exprs/` cluster — approximately
1,600 LOC of expr translators.

`declare.rs:24–207` does the same for declarations: `declare_var` / `declare_var_named`
call `Int::new_const`, `Bool::new_const`, `Array::new_const`, `Set::new_const`, and
`DatatypeBuilder::new` directly. These produce live `Var<'ctx>` entries. A text-emit path
would instead emit `(declare-const name Int)` / `(declare-fun ...)` etc., and the env would
store name strings rather than handles. This is a fundamental representation change.

`datatypes.rs:12–147` builds `DatatypeSort<'static>` at load time via `DatatypeBuilder` and
caches the live sort in `DatatypeRegistry`. In a text-emit path, the sort-building step would
instead emit `(declare-datatype mk_UserType ...)` once at load time. This is a moderate
rewrite (147 LOC) but changes the shared datatype registry from sort-centric to text-centric.

### Mid-translation solver queries — the hardest seam blocker

`inline/guards.rs:17-35` (`guard_is_satisfiable`) runs a full Z3 satisfiability query
INSIDE the inlining loop:

```
solver.push();
solver.assert(g);
let result = solver.check();   // ← live solver query during translation
solver.pop(1);
```

This is invoked from `inline/walk.rs:176` before every `Passthrough` expansion, from
`inline/calls.rs:34,122,261` before every claim/tuple/positional invocation, and from
`inline/subschema.rs:136` before every ∀-over-subclaim. Its purpose is to prune
branches guarded by unsatisfiable conditions (a dead-code elimination optimization). A
text-emit emitter cannot do this — it has no solver to check against. The choices are:
(1) eliminate the optimization (all branches always emitted — correctness preserved,
potentially more assertions); or (2) make the guard-satisfiability check a separate
pre-pass that annotates dead branches before the text emitter runs (architecturally clean
but requires an additional solve phase before emit). This is the single deepest coupling
between the inlining/translation path and the live solver.

`inline/membership.rs:139-145` reads `var.as_datatype_seq()` / `var.as_seq()` to get
live pinned `Int` values for sequence lengths, which are then used to unroll per-element
constraints at translation time. In a text-emit path, this would read from a pre-computed
length map (from `preprocess.rs`) rather than querying the live Z3 handle's `.simplify().as_i64()`.

`exprs/string_ops.rs:23-28` uses raw `z3_sys` C API calls (`Z3_mk_seq_length`,
`Z3_mk_seq_extract`, `Z3_mk_seq_at`, `Z3_mk_seq_index`, `Z3_mk_seq_replace`,
`Z3_str_to_int`, `Z3_int_to_str`) because the z3 crate wrapper doesn't expose them. Each
has a corresponding SMT-LIB form (`str.len`, `str.substr`, etc.) but replacing the C calls
with text emit is straightforward — just change the output.

`inline/dispatch.rs:70-116` calls `var.as_seq()` / `var.as_datatype_seq()` to read pinned
Seq lengths for `resolve_forall_unroll` — the `coindexed` and bare-identifier unroll paths
read `len.simplify().as_i64()` from the live Z3 `Int` handle. This couples dispatch-name
resolution to live Z3 state; in a text-emit path, the Seq lengths would come from the
preprocess metadata map (already computed by `preprocess.rs`).

`eval/decompose.rs:188-277` calls `solver.push()`, `solver.check()`, `solver.pop(1)` per
component in the functional classification loop — entirely engine-side, no seam issue.

### encode_ast / decode_ast: the self-hosting seam

`encode_ast.rs` is a special case: it serializes a Rust `Program` AST as a live Z3
`Datatype` value (matching the `stdlib/ast.ev` enum layout), then pins it into the solver
via `solver.assert(ast._eq(&encoded_value))`. This is how self-hosted Evident passes
receive the input program. In a text-seam world, the analogous operation would be to
serialize the AST as an SMT-LIB constant declaration (a deeply nested Cons chain literal in
text form). The structure is known but the text would be enormous for realistic programs.
The current self-hosting infrastructure (`evaluate_with_extra_assertion`, `eval/extra.rs`)
is entirely engine-side.

`decode_ast.rs` works on `Value::Enum` (a Rust struct produced by the model decoder) —
it has NO Z3 coupling and is already front-end-safe. It would be unchanged by the split.

### Verdict: front-end emitting full-language SMT-LIB text

This is a **deep rewrite, not a moderate port**. Rough quantification:

- `exprs/` cluster: ~1,660 LOC — all 9 files need complete signature + return-type rewrites.
- `inline/` cluster: ~1,160 LOC — walk, calls, membership, subschema need rewrites;
  `guards.rs` needs architectural rethinking (prune-by-sat vs. always-emit).
- `declare.rs` + `datatypes.rs`: ~413 LOC — representation change from live handles to names/text.
- `eval/` cluster: stays in the engine, no rewrite needed.
- `extract.rs` + `encode_ast.rs`: stay in the engine / are engine-adjacent.
- `preprocess.rs`, `rewrite.rs`, `recursion.rs`, `range.rs`, `decode_ast.rs`: already
  context-free; require only minor adaptation to new env representation.

Total files needing substantive rewrite: ~15 files, ~3,200–3,500 LOC of direct rewrite
(plus the architectural guard-satisfiability decision). The existing `smtlib.rs` (495 LOC)
demonstrates the shape of the text-emit path for the scalar subset and can serve as a
template, but extending it to cover the full language (enums, Seq/Array, quantifier unrolling,
claim inlining, record lifting) requires building out all of the above from scratch.
