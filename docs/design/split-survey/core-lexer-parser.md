# Split survey — core / lexer / parser

## Summary

18 files total (8 core, 1 lexer, 9 parser).

| Class | Count | Files |
|---|---|---|
| front-end | 12 | lexer.rs; all 9 parser/; core/ast.rs, core/seq_helpers.rs, core/api.rs |
| engine | 2 | core/z3_program.rs, core/z3_types.rs |
| entangled | 1 | core/functionizer.rs |
| shared-types (leans engine) | 1 | core/value.rs |
| infrastructure | 2 | core/mod.rs (re-export hub), parser/tests.rs |

**Headline seam findings:**

1. **`core/` does NOT cleanly separate.** `mod.rs` re-exports everything — `ast`,
   `Value`, `Z3Program`, `z3_types` — into a single flat namespace (`crate::core::*`).
   All four groups live side-by-side in the same module; there is no sub-module split
   between "front-end core" and "engine core." Breaking the seam requires splitting
   `core/` into at least two sub-crates or sub-modules.

2. **`z3_program.rs` and `z3_types.rs` hold live Z3 handles and cannot cross a
   serialized boundary as-is.** `Z3Program<'ctx>` carries `Dynamic<'ctx>` / `Bool<'ctx>`,
   and `Var<'ctx>` / `CachedSchema<'ctx>` carry typed Z3 AST nodes plus `Solver<'ctx>`.
   Both are lifetime-bound to a `Context`. They cannot be serialized, cloned across
   processes, or passed over a text boundary without replacement.

3. **`core/ast.rs` and `core/value.rs` are clean data types.** `ast.rs` has zero
   Z3 imports. `Value` has zero Z3 imports. Both are `#[derive(Clone)]` pure-data. They
   can cross any serialized boundary (e.g. JSON/bincode) without modification.

4. **`core/functionizer.rs` is entangled**: the `Functionizer` trait compiles a
   `Z3Program` (engine-side) into a `CompiledFunction` that takes `Value` inputs
   (would be front-end-safe). The trait's `compile` signature takes `&Z3Program`,
   `&EnumRegistry`, `&DatatypeRegistry` — all engine-side types. It cannot live purely
   on the front-end side.

5. **`core/ast.rs` has one runtime-dispatch concern embedded**: the `Effect` enum
   (lines 186–238) and `EffectResult` enum (lines 282–292) are primarily consumed by
   `effect_dispatch.rs` (engine side), not by the parser/translator (front-end).
   They live in `ast.rs` because they are part of the Evident AST representation of
   effects, but logically they straddle the seam — the front-end needs the *shape*
   for translation; the engine needs the *values* for dispatch. Splitting would require
   either duplicating the enum or keeping it in a shared-types layer.

---

## Per-file classification

| File | LOC | Class | Why | Seam difficulty | Cross-seam coupling |
|---|---|---|---|---|---|
| `core/ast.rs` | 292 | front-end | Pure Rust structs/enums for Evident AST; no Z3 imports | **low** — zero Z3 deps; `#[derive(Clone)]`; serializable as-is | Imported by all of translate/, parser/, runtime/, effect_loop/, subscriptions.rs, fti.rs, event_sources/. The `Effect`/`EffectResult` variants are consumed engine-side too. |
| `core/value.rs` | 45 | shared-types (leans engine) | `Value` is the output type of Z3 model extraction; consumed by both front-end (as literal pins in AST) and engine (model results, functionizer I/O) | **low** — pure data, no Z3 handles, `#[derive(Clone, PartialEq)]` | Imported everywhere that touches query results: effect_loop/, functionize/, translate/, runtime/, z3_eval.rs. Also used in `Z3Step::PreBaked`. |
| `core/z3_program.rs` | 203 | engine | `Z3Program<'ctx>` carries `Dynamic<'ctx>` / `Bool<'ctx>` live Z3 AST handles; lifetime-bound to `Context` | **high** — all fields hold live `z3::ast::*` handles; lifetime parameter prevents serialization | Imported by functionize/ (all variants), z3_eval.rs (re-exports it), fsm_unroll/. Depends on `z3::ast::{Bool, Dynamic}` and `Value`. |
| `core/z3_types.rs` | 196 | engine | `Var<'ctx>` holds typed Z3 AST handles (Int, Bool, Array, Set, Datatype); `CachedSchema<'ctx>` holds a live `Solver<'ctx>`; `DatatypeRegistry`/`EnumRegistry` hold `&'static DatatypeSort<'static>` | **high** — every non-trivial variant of `Var` holds a live Z3 handle or `&'static DatatypeSort`; `CachedSchema.solver` is a live `Solver<'ctx>` | Imported by translate/ (all sub-modules), z3_eval.rs, functionize/, fsm_unroll/. Depends on `z3::{DatatypeSort, Solver}` and `z3::ast::{Array, Bool, Int, Real, Set, Z3Str}`. |
| `core/api.rs` | 29 | front-end | `QueryResult` and `RuntimeError` are pure-data public-API types with no Z3 deps | **low** — trivially serializable; no Z3 | Imported by runtime/, subscriptions.rs (via `QueryResult`, `RuntimeError`). |
| `core/functionizer.rs` | 27 | entangled | `Functionizer` trait compiles `Z3Program` (engine) → `CompiledFunction` that takes `Value` inputs; straddles both sides | **med** — the trait boundary could theoretically be re-expressed in terms of SMT-LIB text, but all concrete impls (cranelift, glsl, symbolic, satisfier) consume `Z3Program` directly | Depends on `Z3Program`, `DatatypeRegistry`, `EnumRegistry` (all engine); exposes `Value` (shared). Imported by functionize/ and re-exported by functionize/mod.rs. |
| `core/seq_helpers.rs` | 16 | front-end | Pure string utilities — `parse_seq_type` / `internal_cons_helper_name`; no deps | **low** — two pure string functions | Imported by translate/ and effect_loop/. |
| `core/mod.rs` | 17 | infrastructure | Re-export hub — flat `pub use` of all sub-modules | **med** — the flat re-export actively obscures the front-end/engine split; a split would require restructuring `pub use` to surface two namespaces | Imports all of core; re-exports to entire codebase. |
| `lexer.rs` | 387 | front-end | Tokenizer — `String` → `Vec<Token>`; pure data, no Z3 deps | **low** — no Z3, no external deps beyond std | Imported only by parser/mod.rs (`crate::lexer::tokenize`). |
| `parser/mod.rs` | 82 | front-end | Parser entry point and `Parser` struct; delegates to sub-modules | **low** — `parse(src) -> Result<Program>` is a clean pure function; no Z3 | Imports `crate::core::ast::*` and `crate::lexer::Token`. |
| `parser/atoms.rs` | 119 | front-end | Atom-level parsing (literals, match, calls, seq/set/range literals) | **low** | Imports `super::*` (ast + Token). |
| `parser/body_item.rs` | 277 | front-end | Body-item parsing (Membership, ClaimCall, Constraint, Passthrough, Subclaim) | **low** | Imports `super::*`. |
| `parser/exprs.rs` | 326 | front-end | Expression parsing via precedence climbing (quantifiers, implies, ternary, compare, arithmetic) | **low** | Imports `super::*`. |
| `parser/patterns.rs` | 83 | front-end | Match expression and `e matches Pattern` parsing | **low** | Imports `super::*`, `crate::core::ast::MatchPattern`. |
| `parser/program.rs` | 174 | front-end | Top-level `Program` parsing: schemas, enums, imports | **low** | Imports `super::*`. |
| `parser/schema.rs` | 180 | front-end | Schema/claim/type/subclaim/fsm declaration parsing; first-line params; generic type params | **low** | Imports `super::*`. |
| `parser/types.rs` | 140 | front-end | Type-name and pin-clause parsing; generic arg suffix | **low** | Imports `super::*`. |
| `parser/tests.rs` | 261 | infrastructure | Unit tests for parser; no Z3 | **low** | Imports `super::*`. |

---

## Seam notes

### The clean half: lexer + parser + pure AST/api

`lexer.rs` and `parser/` are textbook front-end. The parser produces a `Program`
(from `core/ast.rs`), imports only `crate::core::ast::*` and `crate::lexer::Token`,
and has zero Z3 surface. Moving these into a `front-end` crate is mechanical —
the only dependency they need to bring with them is `core/ast.rs`.

`core/api.rs` (`QueryResult`, `RuntimeError`) and `core/seq_helpers.rs` are likewise
clean; they have no Z3 deps and could live in either a shared-types crate or the
front-end.

### The hard half: Z3 handles cannot cross the seam

`core/z3_program.rs:4` imports `z3::ast::{Bool, Dynamic}` and all of `Z3Program`'s
fields are `Dynamic<'ctx>` or `Bool<'ctx>`. These are live pointers into a Z3
`Context`; they cannot be serialized, cloned across crate boundaries, or passed
over the text boundary the split requires.

`core/z3_types.rs:6–7` imports the full set of typed Z3 AST types plus `Solver<'ctx>`
and `DatatypeSort<'static>`. `CachedSchema<'ctx>` at line 191 holds a live
`Solver<'ctx>`. `Var<'ctx>` has 11 variants; all but `PinnedInt` and `EnumCtor`
hold live Z3 AST handles.

These two files are **engine-only**; they will not appear in a front-end crate. The
SMT-LIB split requires that the front-end emit text and the engine build its own
`Context` + `Var` environment from that text — the `Var`/`CachedSchema` world is
reconstructed on the engine side from parsed SMT-LIB declarations, not passed over.

### The entangled case: `functionizer.rs`

`core/functionizer.rs:7` imports `Z3Program`, `DatatypeRegistry`, `EnumRegistry` —
all engine-side. The `Functionizer` trait is therefore engine-side even though its
output (`CompiledFunction` that takes `Value`) would be compatible with the front-end.

In the split world, `Functionizer` lives entirely in the engine crate. No change
needed to the trait itself; it just stops being re-exported to the front-end.

### The structural problem: `core/mod.rs` flat re-export

`core/mod.rs` uses `pub use value::*`, `pub use z3_types::*`, `pub use z3_program::*`,
`pub use api::*`, `pub use functionizer::*`, `pub use seq_helpers::*`. This means
`crate::core::Var` and `crate::core::Z3Program` live in the same namespace as
`crate::core::Value` and `crate::core::QueryResult`. The entire codebase imports
`crate::core::*` freely and mixes front-end and engine types in single `use` lines
(e.g. `translate/preprocess.rs:7`: `use crate::core::{Value, Var}`).

The recommended split: keep `core/ast.rs`, `core/value.rs`, `core/api.rs`,
`core/seq_helpers.rs` in a new `evident-core` (or `evident-frontend`) crate.
Move `core/z3_program.rs`, `core/z3_types.rs`, `core/functionizer.rs` into the
engine crate. Update `mod.rs` accordingly. This will require updating ~60 `use`
lines across the codebase but all changes are mechanical substitutions.

### Is the AST→SMT-LIB boundary expressible with only front-end core types?

**Yes.** The existing `translate/smtlib.rs` prototype demonstrates this: it takes
`&SchemaDecl` (from `core/ast.rs`) and produces `String` text. The only types
needed at that boundary are:

- `core/ast.rs`: `SchemaDecl`, `BodyItem`, `Expr`, `BinOp`, `Pins`, `Mapping`,
  `EnumDecl`, `Keyword`, `Program`
- `core/value.rs`: `Value` (for literal-pin encoding into SMT-LIB `assert` constants)
- `core/seq_helpers.rs`: `parse_seq_type` (string utility)
- `core/api.rs`: `RuntimeError` (error reporting)

**None** of `Var`, `Z3Program`, `CachedSchema`, `DatatypeRegistry`, or `EnumRegistry`
are needed on the front-end side of the seam. The translate→SMT-LIB path needs
type metadata (what sort does `x` have?) — today that is accumulated in `Var`/`EnumRegistry`
during translation. In the split design, type-sort metadata for the emitted SMT-LIB
can be computed from the AST alone (field declarations carry type names as strings;
enum declarations carry variant shapes as strings) and emitted as SMT-LIB `declare-sort`
/ `declare-datatypes` / `declare-const` statements. The engine then re-parses those
declarations to build its own `Var`/`EnumRegistry` from the SMT-LIB text natively.

The **one non-trivial design question** is the FSM metadata channel: after emitting
SMT-LIB text, the front-end must also communicate which Z3 constant names are
`state`/`state_next`/`effects`/`last_results`/`halt` (so the engine can thread
them correctly). Today this is encoded in the `BodyItem` walk in `effect_loop/fsm.rs`.
In the split design this metadata would be serialized alongside the SMT-LIB text
(e.g. as a small JSON sidecar). The types needed for this — `Keyword`, field-name
strings — are all in `core/ast.rs` and require no Z3.
