# Self-Hosting: Current State (as of commit `a86a1b0`)

Snapshot of what's shipped, what works, what's measured, and what's
ahead. Useful as a recovery point if context is lost — read this
plus `self-hosting-compiler-passes.md` (vision) and
`self-hosting-roadmap.md` (post-Stage 7 plan) to come back up to
speed quickly.

## What ships today

The pipeline:

```
user file
  → Rust parser → Rust ast::Program
                → encode_program_value() → Z3 Datatype value
                → injected as `given` into a self-hosted pass
                → pass runs as a normal Z3 query
                → bindings read out of the model
                → applied to runtime via add_membership_to_claim
                → user's actual query proceeds with augmented body
```

Five `.ev` pass files in `stdlib/passes/`:

| File | Rules | Purpose |
|---|---|---|
| `literal_types.ev` | 7 | Pattern-match single-claim Programs with literal-equality bodies |
| `iter_types.ev` | 4 | Iterate `body ∈ Seq(BodyItem)` to find Memberships / literal assignments |
| `propagation.ev` | 3 | Cross-body-item: `x = y ∧ y = "lit"` → `x ∈ String` |
| `consistency.ev` | 4 | Catch user bugs: `x ∈ String ; x = 5` |
| `lint_duplicate_decls.ev` | 1 | Find two `BIMembership` items with the same name |

Two CLI subcommands surface this:

- `evident infer-types <file> [--strict]` — runs all rules, prints aggregated `Inferred types:` table
- `evident lint <file>` — runs lint rules, exit 5 on findings

The other commands (`query`, `sample`, `execute`) run inference **by default**; `--strict` opts out.

## Runtime API surface (added for self-hosting)

In `runtime-rust/src/runtime.rs`:

```rust
// Encoder
encode_program_value() → Result<Datatype<'static>, EncodeError>
encode_empty_program_value() (private) → cheap MakeProgram(SchLNil, EDLNil)

// System / user boundary
mark_system_loads_complete()        // snapshot what's loaded so far as "system"
user_program() (private)            // Program filtered to user-loaded only
user_claim_count() / user_claim_name(idx)
user_claim_indices_in_file(&Path) → Vec<usize>   // restrict to direct file (Stage 11+ perf)

// Query variants for self-hosted passes
query_with_program(claim, program_var)                         // encodes once internally
query_with_program_value(claim, program_var, prog_value)       // cached encoder path
query_with_program_and_nth_claim_body(claim, prog_var, body_var, idx)
query_with_program_and_nth_claim_body_value(claim, prog_var, body_var, idx, prog_value)
query_with_nth_claim_body_only(claim, body_var, idx)           // skips Program — body-only path

// Membership injection (used by --strict-default inference)
add_membership_to_claim(claim, var, type_name) → Result<bool>  // returns false if already declared
```

In `runtime-rust/src/translate/encode_ast.rs`:

```rust
encode_program(&Program, ctx, enums) → Result<Datatype<'static>>
encode_body_items_into_seq(&[BodyItem], arr, len, ctx, enums) → Vec<Bool<'ctx>>
```

Plus per-AST-node encoders called recursively by `encode_program`.

## stdlib/ast.ev — the canonical AST shape

17 mutually-recursive enums built via Z3's `create_datatypes`:

- `Program = MakeProgram(SchemaList, EnumDeclList)`
- `SchemaDecl = MakeSchemaDecl(Keyword, String, BodyItemList)`
- `Keyword = KSchema | KClaim | KType | KSubclaim`
- `BodyItem = BIMembership(String, String, Pins) | BIPassthrough(String) | BIClaimCall(String, MappingList) | BIConstraint(Expr) | BISubclaim(SchemaDecl)`
- `Pins = PNone | PNamed(MappingList) | PPositional(ExprList)`
- `Mapping = MakeMapping(String, Expr)`
- `Expr = EIdentifier(String) | EInt(Int) | EReal(Real) | EBool(Bool) | EStr(String) | ESetLit(ExprList) | ESeqLit(ExprList) | ERange(Expr, Expr) | EInExpr(Expr, Expr) | EForall(StringList, Expr, Expr) | EExists(StringList, Expr, Expr) | ECall(String, ExprList) | ECardinality(Expr) | EIndex(Expr, Expr) | EField(Expr, String) | EBinary(BinOp, Expr, Expr) | ENot(Expr)`
- `BinOp = OpEq | OpNeq | OpLt | OpLe | OpGt | OpGe | OpAnd | OpOr | OpImplies | OpAdd | OpSub | OpMul | OpDiv | OpConcat`
- `EnumDecl = MakeEnumDecl(String, EnumVariantList)`
- `EnumVariant = MakeEnumVariant(String, EnumFieldList)`
- `EnumField = MakeEnumField(String, String)`
- 8 list types: `BodyItemList`, `ExprList`, `MappingList`, `SchemaList`, `EnumDeclList`, `EnumVariantList`, `EnumFieldList`, `StringList`. All Nil/Cons recursive enums.

## Performance — measured on `programs/mario/mario_shader.ev`

This is a real-world workload — multi-claim program with imports
(engine.ev, level_data.ev — 26 user-visible claims after expansion).

| Stage | execute time | Inference overhead | Notes |
|---|---|---|---|
| Initial Stage 12 | 2.6s | ~2.1s | Encoder ran per rule; full Program asserted for every rule |
| + body-only injection | 1.0s | ~0.5s (4x) | Most rules don't read `program`; skip the deep equality |
| + direct-claims-only | 0.73s | ~0.2s (10x) | Iterate user's 1-3 claims, not 26 imported |
| `--strict` baseline | 0.52s | 0 | No inference |

The remaining ~200ms is split:
- Inference runtime setup (load `stdlib/ast.ev` + 4 pass files in a separate Z3 context): ~150ms. The Z3 datatype build for the 17-enum bundle dominates.
- Rule queries on user's claims: ~50ms.

Further reduction would need cross-process state sharing (daemon mode) — large scope, not pursued.

## What's blocking further self-hosting

**The decoder.** We have:

- ✅ Rust AST → Z3 Datatype value (the encoder)
- ❌ Z3 model → Rust AST (the decoder)

Inference / lint passes only need to extract simple facts from bindings (var, type, claim_name as `Value::Str` etc.). They don't reconstruct programs. So the decoder gap doesn't affect them.

But to **migrate a Rust desugar to Evident**, the pass must produce a transformed Program that the runtime then loads and uses. That requires the decoder.

The decoder shape:

```rust
// In runtime-rust/src/translate/decode_ast.rs (new module):
pub fn decode_program(value: &Value, enums: &EnumRegistry) → Result<Program, DecodeError>
pub fn decode_schema_decl(value: &Value, ...) → Result<SchemaDecl, _>
pub fn decode_body_item(value: &Value, ...) → Result<BodyItem, _>
pub fn decode_expr(value: &Value, ...) → Result<Expr, _>
// etc.
```

`Value::Enum { enum_name, variant, fields: Vec<Value> }` is what the model extractor already produces. The decoder pattern-matches on `(enum_name, variant)` and recursively decodes each field. Mechanical, similar shape to the encoder. Estimate: ~200 lines.

## Migration candidates (Rust → Evident, after decoder)

Listed in order of estimated payoff. Each requires the decoder + a corresponding `stdlib/passes/desugar_*.ev` file.

| Rust code | Lines saved | Pass file size estimate | Net |
|---|---|---|---|
| `parser.rs:try_parse_chained_membership` | ~150 | ~50 | +100 |
| `exprs.rs` record-lift (componentwise `=`/`<`/etc.) | ~250 | ~100 | +150 |
| `exprs.rs` `coindexed`/`edges` quantifier expansion | ~150 | ~80 | +70 |
| `parser.rs` chained comparison | ~80 | ~40 | +40 |
| `declare.rs` sub-record expansion | ~80 | ~50 | +30 |
| `parser.rs` multi-name (`x, y ∈ Int`) | ~50 | ~30 | +20 |
| `parser.rs` implies-block / quantifier-block | ~70 | ~40 | +30 |
| `inline.rs` bare-identifier-as-passthrough | ~40 | ~30 | +10 |
| `inline.rs` first-line params | ~60 | ~30 | +30 |

Total Rust removable: ~930 lines. Total Evident added: ~450 lines. Net: ~480 line reduction. Plus ~200 line decoder amortized across all migrations.

## Migration recommendation

**Start with `chained_membership`.** Most recent (best understood), well-bounded, pure AST transformation, and the smallest single file (~150 Rust → ~50 Evident). One commit teaches us:

1. What the decoder looks like in practice.
2. Whether the migration shape (parse → run pass → decode result → re-load) is as clean as it sounds, or has subtle issues (variable binding, source-location tracking, error messages, etc.).
3. The actual perf cost of running a desugar pass at parse time.

If that goes well, the remaining migrations follow the same pattern with diminishing per-task cost. If it's painful, we have one decoder + one self-hosted desugar — still useful, just not the line-count reduction we hoped for.

## Things to watch during the chained-membership migration

- **Source location preservation.** The Rust parser tracks line/col for error messages. The encoder doesn't. After round-tripping through a desugar pass, source locations may be lost. Either (a) accept worse error messages on desugared code, (b) carry locations through the encoder/decoder.
- **Performance.** Running a desugar pass at parse time means every program load now includes ~200ms+ of inference-style setup. For `evident execute mario_shader.ev` that's already paid (we run inference anyway). For `evident query` on a tiny file, this could double the load time.
- **Correctness.** Behavior must match the existing Rust desugar exactly. Use the existing parser unit tests (`parse_chained_membership_*`) as the conformance set — switch to the Evident desugar, all tests must still pass.
- **Plumbing.** The migration requires either (a) loading the desugar pass into the main runtime alongside ast.ev and pass files, or (b) running it in a separate runtime and re-loading the result. (b) is cleaner but may have subtle issues with shared types (e.g. enum variant uniqueness across runtimes).

## Optimization journey reference

For posterity, the four perf-tuning commits:

| Commit | Change | mario_shader |
|---|---|---|
| `e767b52` | `--infer-types` default ON | 2.6s execute |
| `32a50b2` | Body-only injection + cached encoder | 1.0s |
| `a86a1b0` | Direct-claims-only iteration | 0.73s |

What didn't help (within noise):
- Caching the encoded Program (alone, without body-only): ~50ms savings
- Skipping `PROGRAM_RULES` on multi-claim: ~50ms savings
- (Both kept anyway, low-risk consolidation)

What we considered but didn't do:
- **Parallelism**: Z3's safe Rust API has `Solver` as `!Sync`. Per-thread contexts would each have to load `stdlib/ast.ev` + 4 pass files separately. Setup cost would dwarf any wall-time savings on workloads mario_shader's size.
- **Daemon mode** (cache state across CLI invocations): large scope. Out of scope for now.
- **`fully_typed` shortcut** (skip inference when no undeclared vars): user preference was to keep consistent overhead and not have a special-case codepath.

## Files to know

| Path | Purpose |
|---|---|
| `stdlib/ast.ev` | Canonical AST as Evident enums |
| `stdlib/passes/*.ev` | All self-hosted compiler passes |
| `runtime-rust/src/translate/encode_ast.rs` | Rust AST → Z3 Datatype value |
| `runtime-rust/src/runtime.rs` | EvidentRuntime API including all the `query_with_program*` variants |
| `runtime-rust/src/commands/infer_types.rs` | The inference pipeline (`collect_inferences`, `auto_apply_inferences`) |
| `runtime-rust/src/commands/lint.rs` | The lint subcommand |
| `runtime-rust/tests/encode_ast.rs` | 29 encoder tests |
| `runtime-rust/tests/iter_pass.rs` | 16 iteration tests |
| `runtime-rust/tests/propagation_pass.rs` | 8 propagation tests |
| `runtime-rust/tests/self_hosted_pass.rs` | 24 plumbing tests |
| `programs/lang_tests/test_pass_*.ev` | Hand-built `.ev` conformance tests for each pass |
| `programs/lang_tests/test_seq_of_enum.ev` | Stage 5 conformance |
| `docs/design/self-hosting-compiler-passes.md` | Original vision doc |
| `docs/design/self-hosting-roadmap.md` | Post-Stage 7 plan (now closed) |
| `docs/tutorials/writing-a-pass.md` | How-to for adding a new pass |
| `docs/rust-runtime-capabilities.md` | Reverse-engineered runtime reference |

## Test counts (regression baseline)

- 387 rust tests (cargo test --release)
- 221 program tests (`evident test programs/`)
- 2 pre-existing parse errors in `programs/adventure*` — known-bad, not from this work

## Vocabulary established during this work

Worth knowing for any continuation:

- **Self-hosted pass** — a compiler pass written as Evident `.ev` claims, not Rust code. Receives parsed Program as a Z3 datatype value via `given`; emits constraints over output variables.
- **Body-only path** — passes that declare `program ∈ Program` but never reference it (iter_types, propagation, consistency, lint_duplicate_decls). Runtime injects an empty Program for those, skipping the expensive deep-equality assertion.
- **System boundary** — `mark_system_loads_complete()` snapshot dividing "stuff loaded by the framework" from "user's actual program." `user_program()` filters to user-loaded only.
- **`--strict`** — flag on `query`/`sample`/`execute` to skip the inference pre-pass. Different meaning than `infer-types --strict` (which is "exit non-zero on conflicts").
- **Inference vs. lint vs. consistency** — three flavors of pass, all built on the same machinery:
  - Inference: SAT means "I found a fact." Bindings are the result.
  - Lint / consistency: SAT means "I found a problem." Print a diagnostic.
- **Direct-claims-only** — restrict iteration to schemas whose source file matches the user's CLI argument; skip transitively-imported claims (typically library helpers).
