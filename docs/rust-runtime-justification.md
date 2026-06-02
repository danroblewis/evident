# Rust runtime — file-by-file justification (post-shrink)

Branch: `rust-runtime-shrink`.
Size: **9,787 lines** across **56 `.rs` files** in `runtime/src/`.

(Earlier audit at 18,666 LOC / 94 files was executed end-to-end —
see commits `8929f27`, `7fe6d98`, `fb2eb74`, `7c36bc7`. This is the
re-audit on the current state.)

## Framing

Three responsibilities:

1. **Language frontend** — lexer, parser, AST (the source-text → IR pipeline)
2. **Claim composition** — passthrough, names-match, explicit `↦`, guarded `⇒`,
   tuple-in-claim, subclaim dispatch, chained membership
3. **Z3 model construction** — declare leaves, translate exprs, evaluate, extract

Every file in the runtime now serves one of these three. The shrink
removed everything that didn't fit — multi-FSM scheduler, async event
sources, effect dispatch, FFI, self-hosted passes, reflection,
autotune, the AST decoder used by the now-deleted self-hosted passes.

## Per-file table

Sorted by group, then by lines (desc).

### Group A — Language frontend (parsing → AST)

**Total: ~2,200 LOC, 13 files.** All KEEP. Each maps to a specific
piece of the language grammar.

| Path | LOC | What it does |
|---|---:|---|
| `lexer.rs`              | 360 | Unicode operators + word-keywords → tokens |
| `parser/exprs.rs`       | 326 | Expression parsing (precedence climb) |
| `parser/body_item.rs`   | 277 | Membership / passthrough / claim-call / subclaim / constraint |
| `parser/schema.rs`      | 180 | Schema header + first-line params |
| `parser/program.rs`     | 174 | Top-level: imports + schema list + enum decls |
| `parser/types.rs`       | 140 | Type-name parsing (Seq(T), generics, …) |
| `parser/atoms.rs`       | 115 | Literals, identifiers, parens |
| `parser/patterns.rs`    |  83 | Match-pattern parsing |
| `parser/mod.rs`         |  79 | Parser struct, `parse()` entry |
| `core/ast.rs`           | 170 | AST node types (Expr, BodyItem, SchemaDecl, …) |
| `core/value.rs`         |  42 | `Value` + `EvalResult` |
| `core/api.rs`           |  29 | `QueryResult` + `RuntimeError` |
| `core/seq_helpers.rs`   |  16 | `parse_seq_type` / Cons-helper-name string utils |
| `core/mod.rs`           |  13 | Re-exports |

### Group B — Claim composition (inline)

**Total: ~1,300 LOC, 8 files.** All KEEP. This is responsibility (2)
in full. Splitting these tighter (e.g. merging calls.rs + dispatch.rs)
is possible but each file already has a single concern.

| Path | LOC | What it does |
|---|---:|---|
| `translate/inline/calls.rs`      | 309 | `ClaimName(slot ↦ val)`, `(args) ∈ claim`, guarded `⇒`, names-match |
| `translate/inline/dispatch.rs`   | 204 | Receiver-prefix resolution, method dispatch, forall-unroll |
| `translate/inline/subschema.rs`  | 179 | `recv.subclaim(args)` invocation, field rebinding |
| `translate/inline/walk.rs`       | 176 | Body-item walker; routes each item to its handler |
| `translate/inline/membership.rs` | 170 | `x ∈ Type(...)` — declare + apply named/positional pins |
| `translate/inline/rewrite.rs`    | 152 | Prefix-injection + bound-var substitution |
| `translate/inline/recursion.rs`  |  59 | Per-claim depth counter; helper-local Z3-const isolation |
| `translate/inline/guards.rs`     |  56 | Solver-assertion + guard-composition helpers |
| `translate/inline/mod.rs`        |  13 | Re-exports |

### Group C — Z3 model construction (expressions)

**Total: ~2,700 LOC, 11 files.** All KEEP. One file per
expression-kind family. The two larger ones (`seq_eq.rs` at 485,
`bool.rs` at 415) carry weight because Seq equality and Bool
translation are the densest part of the surface.

| Path | LOC | What it does |
|---|---:|---|
| `translate/exprs/seq_eq.rs`      | 485 | Seq equality / Cons-chain lowering / SeqLit pinning |
| `translate/exprs/bool.rs`        | 415 | Bool expressions (comparisons, ∧/∨/⇒, ∈, ∀/∃, …) |
| `translate/exprs/record_lift.rs` | 290 | Componentwise lift for record types (`Vec2 < Vec2`, `c = a - b`) |
| `translate/exprs/scalar.rs`      | 256 | Int / Real / String identifier + literal translation |
| `translate/exprs/enums.rs`       | 253 | Enum constructor / variant Expr → Z3 Datatype apply |
| `translate/exprs/quant.rs`       | 250 | `Forall` / `Exists` with pinned ranges + tuple destructuring |
| `translate/exprs/mapping.rs`     | 236 | Pin-mapping resolution (`name ↦ value`) |
| `translate/exprs/string_ops.rs`  | 193 | str_len, index_of, substr, replace, starts_with, … |
| `translate/exprs/match_expr.rs`  | 183 | `match` expression lowering (nested ITE) |
| `translate/exprs/seq_field.rs`   | 159 | `Seq(Composite).field` access |
| `translate/exprs/range.rs`       |  28 | `{lo..hi}` for quantifier bounds |
| `translate/exprs/mod.rs`         |  79 | Thread-local enum-registry guard + SeqLit-target hint |

### Group D — Z3 model construction (engine)

**Total: ~2,000 LOC, 8 files.** All KEEP. The pipeline that turns
an `Expr`/`BodyItem` tree into asserted Z3 constraints + extracts
model values.

| Path | LOC | What it does |
|---|---:|---|
| `translate/extract.rs`         | 489 | Z3 model → `Value`; pin Seq/Set from `given`; `z3_string` Unicode escape |
| `translate/preprocess.rs`      | 370 | Pin literal-int vars, propagate Seq lengths, fold quantifier bounds |
| `translate/declare.rs`         | 266 | Declare Z3 leaves per type (Int/Bool/Real/Str/Seq/Set/Datatype/Enum) |
| `translate/eval/decode.rs`     | 213 | `extract_enum_value` — Z3 model → Value::Enum (variant + payload) |
| `translate/eval/mod.rs`        | 179 | THE canonical `evaluate` entry — 3-pass solve |
| `translate/eval/cached.rs`     | 164 | Cached-solver path (push/pop, structural signature) |
| `translate/datatypes.rs`       | 147 | `Seq(UserType)` DatatypeSort caching |
| `translate/eval/solver.rs`     | 122 | `make_tuned_solver`, `declare_and_assert`, real helpers |
| `translate/encode_ast.rs`      |  51 | `value_enum_to_datatype` — pin enum-typed `given` value |
| `translate.rs`                 |  15 | Module decls + re-exports |

### Group E — Runtime API & passes

**Total: ~900 LOC, 7 files.** All KEEP. The top-level `EvidentRuntime`
type, load+query entry points, and the three pre-translation passes
(`++` flattening, type inference, generics monomorphization).

| Path | LOC | What it does |
|---|---:|---|
| `runtime/register_enums.rs`   | 420 | Z3 datatype registration (recursive, mutual, multi-stage topo) |
| `runtime/inject.rs`           | 283 | Type inference: `inject_claim_arg_types`, `inject_lhs_eq_types` |
| `runtime/generics.rs`         | 191 | Generic monomorphization (`Edge<Rect>` → concrete schema), fixpoint |
| `runtime/desugar.rs`          |  86 | `++` Seq concat flattening (the only pre-translation desugar left) |
| `runtime/mod.rs`              |  79 | `EvidentRuntime` struct + getter methods |
| `runtime/load.rs`             |  78 | Parse + run passes + register subclaims + flush caches |
| `runtime/query.rs`            |  52 | `query` + `query_cached` (public API) |

### Group F — Shared types & glue

**Total: ~330 LOC, 5 files.** All KEEP. Types every other group
imports, plus the CLI binary.

| Path | LOC | What it does |
|---|---:|---|
| `core/z3_types.rs`            | 196 | `Var`, `FieldKind`, `SeqFieldElem`, `EnumRegistry`, `CachedSchema` |
| `main.rs`                     | 172 | CLI: `sample <file> [<claim>] [-n N] [--given …] [--all] [--json]` |
| `z3_ctx.rs`                   |  48 | Global Mutex around Z3 Context creation (thread safety) |
| `lib.rs`                      |  17 | Public surface re-exports |
| `core/mod.rs` (counted above) |     | |

## Possible further shrink (none aggressive)

| Where | Approx | What it would take |
|---|---:|---|
| `core/z3_types.rs` `Var` enum | ~30 LOC | If `DatatypeSetVar` is never instantiated by any path, drop it + its accessors. Verify by tracing `Set(UserType)` declarations. |
| `translate/eval/cached.rs` | ~40 LOC | Cached path duplicates a lot of `eval/mod.rs`'s setup. A shared helper + 2 thin wrappers would deduplicate. Risk: pulls the structural-signature gating into the canonical path. |
| `translate/exprs/mod.rs` | ~30 LOC | `EnumRegistryGuard` + `with_target_enum_hint` are thread-local coupling smells. Could thread the registry through `ctx` instead. Higher churn, less LOC saved. |
| `runtime/inject.rs` | ~50 LOC | Two passes share a lot of expression-walk plumbing. A pattern-shared walker + per-pass visitor would cut some, but the passes have asymmetric concerns. |
| `core/ast.rs` | ~10 LOC | `BODY_MARKERS` (`spawnable_only`) was a multi-FSM tag; safe to drop now. |

None of these is a big win, and several are anti-shrink (introducing
shared helpers adds indirection). The runtime is at its natural floor
for the supported language scope.

## To shrink further you have to cut language

Going below ~8K LOC requires removing language features:

| Remove | Approx saved |
|---|---:|
| `match` + `matches`              | ~270 LOC (`exprs/match_expr.rs` + `parser/patterns.rs`) |
| Generics (`type Edge<T>`)        | ~190 LOC (`runtime/generics.rs`) — but tests + 4 lang-test claims fail |
| `Set` (`SetLit`, `Set(T)`)       | ~150 LOC across declare + extract + Var variants |
| `Real`                           | ~80 LOC scattered across translate + extract |
| `String` ops (`substr`, …)       | ~193 LOC (`exprs/string_ops.rs`) |

Each cut is a language-scope decision. The current scope is
documented in `tests/lang_tests/`; if a feature there doesn't earn
its keep, remove the test claims and the code together.

## Test surface

- `tests/conformance/` — 131 black-box CLI tests (Python).
- `tests/lang_tests/` — 10 files, 146 `sat_*`/`unsat_*` claims.

Both run from `./test.sh` in ~3s and validate the full language
contract. Any further shrink should keep both green.
