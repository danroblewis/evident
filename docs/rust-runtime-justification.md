# Rust runtime — file-by-file justification

Branch: `rust-runtime-shrink`.
Starting size: **18,666 lines** across **94 `.rs` files** in `runtime/src/`.

## STATUS — EXECUTED

This audit was executed end-to-end (see commits `8929f27`, `7fe6d98`,
`fb2eb74`). Final size: **9,787 lines across 56 files**, a 47.6%
reduction from the starting point. All REMOVE-category files are
gone, REWORK files were either trimmed or removed where the role
no longer survived in the minimal runtime, and the projected ~8,500
target is within 15% (the residual is in three KEEP files that
contain genuinely needed language-semantics code:
`translate/extract.rs` 489 LOC, `translate/exprs/seq_eq.rs` 485,
and `runtime/register_enums.rs` 420 — further shrink there would
require cutting language features, out of scope).

CLI is one subcommand (`evident sample <file> [<claim>]`); test
suite is 131 conformance + 146 lang-test claims, all green.

The remainder of this document is the original per-file plan;
useful as reference for what each surviving file does.

## Framing

The user's invariant for the runtime is **three** responsibilities:

1. Language syntax & semantics — lexer, parser, AST
2. Claim composition — passthrough (`..`), names-match, explicit
   binding (`↦`), pull-up, guarded, tuple-in-claim, subclaim,
   chained membership
3. Z3 model construction — translating AST to SMT-LIB / Z3 AST

Everything else (multi-FSM scheduling, async event sources, FTI
declarative install, world-types, the percolated-effects machinery,
subscription inference, toposort dispatch, autotune, profiling
trace, FFI plumbing, the self-hosted pass runner, the percolated
nested-FSM `run(F, init)` machinery, FTI registry, `effect_dispatch`'s
42-variant Effect handler) is **REMOVE or REWORK by default**.
The prior agent kept it because the existing code depends on it.
This audit treats existing-code-depends is **not** a justification —
the dependency itself is removable if its consumers are removable.

REMOVE = the user has said it should not exist in the Rust runtime.
REWORK = the role survives but the implementation should move to
Evident `.ev` files / FFI / library code.

---

## Per-file table

Sorted by category, then by lines (desc).

### KEEP — load-bearing for the three responsibilities

| Path | Lines | Primary responsibility | Justification |
|---|---:|---|---|
| `translate/encode_ast.rs` | 944 | AST → `Value::Enum` + AST → Z3 Datatype marshaler | THE marshaler. Required by every claim that references AST values (reflection-style composition). Will shrink once subclaim-based passes go away — much of this is for the self-hosted pass infrastructure. **Candidate for shrink, not removal.** |
| `translate/decode_ast.rs` | 854 | Z3 model → Rust AST decoder (`decode_program`, `decode_effect`, `decode_*`) | Inverse of `encode_ast`. Same shrink-vs-keep dynamic. Probably ~60% is for the self-hosted pass runner and can disappear. **KEEP for now; will shrink.** |
| `translate/extract.rs` | 489 | Extract model values from a satisfied Z3 solver; pin Seq/Set from `given`; `z3_string` non-ASCII escape | Core Z3 model decode. Deleting breaks every query that returns bindings. Includes the `#16` Unicode escape — required. |
| `translate/exprs/seq_eq.rs` | 485 | `Seq` equality / Cons-chain lowering | Seq literals, `=` between Seqs, Cons-chain expansion. KEEP. |
| `runtime/register_enums.rs` | 420 | Z3 datatype registration for `enum` declarations | Mandatory before any enum-using claim can be queried. KEEP. |
| `translate/exprs/bool.rs` | 415 | Bool expression translation | KEEP. |
| `translate/preprocess.rs` | 370 | Pin literal-int vars, propagate Seq lengths, fold quantifier bounds | Required for any `∀ i ∈ {0..n-1}` quantifier. KEEP. |
| `lexer.rs` | 360 | Unicode operators + word-keywords → tokens | Responsibility (1). KEEP. |
| `parser/exprs.rs` | 326 | Expression parsing | Responsibility (1). KEEP. |
| `translate/inline/calls.rs` | 309 | Claim invocation inlining (`Claim(...)`, `(args) ∈ claim`, guarded `⇒`, `Claim(slot ↦ val)`) | Responsibility (2) — claim composition. KEEP. |
| `core/ast.rs` | 292 | AST node types | Responsibility (1). KEEP (will shed `HaltsWithin`, `RunFsm`, percolated bits when REMOVE lands). |
| `translate/exprs/record_lift.rs` | 290 | Componentwise lift for record types (Vec2 < Vec2, c = a - b) | The `IVec2/Color` lift the language doc describes. KEEP. |
| `translate/eval/decode.rs` | 279 | Decode enum / composite shapes from a Z3 model | Required for query bindings to include enum values. KEEP. |
| `parser/body_item.rs` | 277 | Parse body items (membership, passthrough, claim call, subclaim, constraint) | Responsibility (1) + (2). KEEP. |
| `translate/eval/extra.rs` | 267 | `evaluate_with_extra_assertion` / `_with_program_and_body` | The "supply extra Datatype pins" path. The `_with_program_and_body` half is reflection-only and shrinks; the simpler one is the canonical solve entry. KEEP, will shrink. |
| `translate/declare.rs` | 266 | Declare Z3 leaves per type (Int/Bool/Real/Str/Seq/Set/Datatype/Enum) | Mandatory pass 1 of every solve. KEEP. |
| `translate/exprs/scalar.rs` | 256 | Translate Int / Real / String identifiers + literals | KEEP. |
| `translate/exprs/enums.rs` | 253 | Enum constructor / variant Expr → Z3 Datatype apply | KEEP. |
| `translate/exprs/quant.rs` | 250 | `Forall`/`Exists` translation with pinned ranges, tuple destructuring | KEEP. |
| `translate/exprs/mapping.rs` | 236 | Pin-mapping resolution (`name ↦ value`, `name ∈ Type (a ↦ v)`) | Claim composition (named binding). KEEP. |
| `translate/inline/dispatch.rs` | 204 | Method-dispatch + receiver-prefix resolution (`recv.subclaim(args)`) | Responsibility (2). KEEP. |
| `core/z3_types.rs` | 196 | `EnumRegistry`, `CachedSchema`, `Var`, `FieldKind`, `SeqFieldElem`, `DatatypeRegistry` | Shared types touched by everything. KEEP. |
| `translate/exprs/string_ops.rs` | 193 | `str_len`, `index_of`, `substr`, `replace`, `starts_with`, ... | Stdlib string ops. Required by string-handling claims. KEEP. |
| `translate/inline/walk.rs` | 184 | Body-item walker (entry into per-item inlining) | KEEP. |
| `translate/exprs/match_expr.rs` | 183 | `match` expression lowering (nested ITE over recognizers) | Language feature (1)+(2). KEEP. |
| `translate/eval/mod.rs` | 182 | `evaluate` canonical entry — 3-pass solve | THE solve entry. KEEP. |
| `parser/schema.rs` | 180 | Schema-header + first-line-params parsing | Responsibility (1). KEEP. |
| `translate/inline/subschema.rs` | 179 | `recv.subclaim(args)` invocation | Responsibility (2). KEEP. |
| `translate/inline/membership.rs` | 176 | `x ∈ Type (...)` — declare + apply named/positional pins | Responsibility (2). KEEP. |
| `parser/program.rs` | 174 | Top-level program parsing | Responsibility (1). KEEP. |
| `translate/eval/cached.rs` | 164 | Cached-solver path (push/pop, structural signature) | Used by `query_cached`. KEEP. |
| `translate/exprs/seq_field.rs` | 159 | `Seq(Composite).field` access translation | Required for `dots[i].pos` style. KEEP. |
| `translate/inline/rewrite.rs` | 155 | Bound-variable substitution into claim bodies | KEEP. |
| `translate/datatypes.rs` | 147 | `Seq(UserType)` datatype-sort caching | KEEP. |
| `parser/types.rs` | 140 | Type-name parsing (`Seq(T)`, generics, etc.) | KEEP. |
| `core/mod.rs` + `core/api.rs` + `core/value.rs` + `core/seq_helpers.rs` | 13+29+45+16 = 103 | `Value`, `QueryResult`, `RuntimeError`, `parse_seq_type` | KEEP. |
| `translate/eval/solver.rs` | 122 | `make_tuned_solver`, `declare_and_assert`, real-number helpers | KEEP. |
| `parser/atoms.rs` | 119 | Atom-level parsing (literals, identifiers, parens) | KEEP. |
| `stdlib_path.rs` | 114 | `EVIDENT_STDLIB` resolution (env → install → dev fallback) | Necessary for the binary to find `stdlib/runtime.ev`. KEEP, may shrink. |
| `runtime/mod.rs` | 134 | `EvidentRuntime` struct + getter methods | KEEP — but `solve_history` (autotune), `slow_parallel_enabled`, `system_boundary`, `schema_origins`, `cache_rebuilds` SHED. |
| `runtime/load.rs` | 145 | Parse + run all pre-translation passes + cache flush | KEEP, but the calls to `portable::inject::*`, `portable::generics::monomorphize`, `super::desugar::*`, `super::validate::enforce_external_only`, `lower_fsm_application`, `validate_run_targets` all go away or move (some to KEEP-in-Rust simple Rust passes, some to user-runs-pass-from-CLI). |
| `translate.rs` | 57 | Module declarations + public re-exports | KEEP. |
| `parser/mod.rs` | 79 | `Parser`, `parse()` entry | KEEP. |
| `parser/patterns.rs` | 83 | Match-pattern parsing | KEEP (match is a kept language feature). |
| `translate/exprs/mod.rs` | 79 | EnumRegistry thread-local guard + SeqLit-target hint | KEEP — but the SeqLit-target hint mechanism is a coupling smell to revisit. |
| `runtime/query.rs` | 75 | `query` + `query_cached` (Z3-only) | THE public query entry. KEEP. The `resolve_runs` call (RunFsm pre-solve) goes away with REMOVE of `run(F, init)`. |
| `translate/inline/recursion.rs` | 59 | Recursion-depth guard + helper-local isolation | KEEP. |
| `translate/inline/guards.rs` | 56 | `cond ⇒ body` guard composition | Responsibility (2). KEEP. |
| `translate/inline/mod.rs` | 13 | Module decls | KEEP. |
| `z3_ctx.rs` | 48 | Global Mutex-serialized Z3 Context creation | Necessary for thread safety of Z3 init. KEEP. |
| `translate/exprs/range.rs` | 28 | `{lo..hi}` range translation | KEEP. |
| `lib.rs` | 28 | Crate root, public re-exports | KEEP. |
| `main.rs` | 24 | CLI entry | KEEP (will be 1 subcommand, smaller). |

**KEEP subtotal: ~ 11,300 lines across ~52 files.**

(Several of these will internally shrink once REMOVE removes their consumers; the real KEEP floor is closer to ~8K.)

---

### REMOVE — multi-FSM machinery, scheduler, async events, world types, run-program-not-language

| Path | Lines | What it is | Justification for REMOVE |
|---|---:|---|---|
| `effect_dispatch.rs` | 639 | Dispatches 42 `Effect` variants (Print/Read/FFI/SpawnFsm/Exit/Mem*/etc.) | The user said multi-FSM machinery goes; this is the central hub. FFI itself is REWORK (one libcall primitive). Everything else (stdin/stdout/ParseInt/ShellRun/SpawnFsm/Exit/IntToStr/RealToStr/Memory IO/CloseHandle/RegisterCallback/MonotonicTime/Malloc) is "running sophisticated programs". REMOVE. |
| `runtime/nested.rs` | 539 | Pre-solve resolution of `run(F, init)` and `halts_within(F, N)`; embeds an FSM as a constraint by driving it to halt then pinning the final state | This is the entire `run(F, init)` percolated-effects infrastructure. The user wants compile-time composition, not blocking interpretation. REMOVE. |
| `effect_loop/scheduler.rs` | 513 | THE subscription-driven multi-FSM scheduler: per-tick solves, wake-set logic, plugin-write drain, world-snapshot merging, SpawnFsm handling, event channel | This is the multi-FSM scheduler in one file. The whole point of REMOVE. REMOVE. |
| `effect_loop/nested.rs` | 331 | Tier-3 blocking interpreter for `run(F, init)` — drives a nested FSM, captures effects, returns final state | Pairs with `runtime/nested.rs` — the nested-FSM execution machinery. REMOVE. |
| `effect_loop/mod.rs` | 312 | `LoopOpts`/`LoopEnv`/`LoopResult`, source bootstrap, single-owner invariant check | The multi-FSM entry. REMOVE. |
| `effect_loop/fsm.rs` | 210 | `MainShape` + `resolve_fsm` walk (fsm-keyword → scheduler record); slot resolution including `world`/`world_next`/marker-type events/FTI params | Multi-FSM concern. REMOVE. |
| `effect_loop/collect.rs` | 198 | Collect dispatchable Effects from a model in mode-1 / mode-2 (toposort dispatch) | Multi-FSM tick output handling. REMOVE. |
| `effect_loop/state.rs` | 65 | `seed_state_with_arg` / `encode_state_value` | Scheduler-only helpers. REMOVE. |
| `effect_loop/toposort.rs` | 40 | Wraps the self-hosted toposort for effect-dispatch ordering | Mode-2 dispatch — multi-FSM concern. REMOVE. |
| `effect_loop/timing.rs` | 32 | Per-tick timing summary printer | Diagnostics for the scheduler. REMOVE. |
| `runtime/scheduler_api.rs` | 60 | `query_with_pins_and_given`, `collect_tick_effects` — per-tick scheduler entry | Multi-FSM API. REMOVE; the underlying `evaluate_with_extra_assertions` stays. |
| `runtime/reflection.rs` | 212 | `encode_program_value`, `program_ast`, `query_with_program_and_nth_claim_body`, system-boundary snapshot | Used to inject the loaded Program AST into self-hosted passes. With the self-hosted pass infra removed, this goes too. REMOVE. |
| `runtime/introspect.rs` | 105 | `add_membership_to_claim`, `replace_body_item_in_claim`, user-claim indexing | Only consumer is `auto_apply_desugar` (passthrough rewrite) which itself uses a self-hosted pass. REMOVE the whole stack. |
| `runtime/autotune.rs` | 97 | `smt.arith.solver` candidate prober (2 vs 6) | Performance tuning of the existing solver path. Not language-defining. Pick a default and delete; if real, move to a one-line env-flag. REMOVE. |
| `runtime/lenient.rs` | 8 | `EVIDENT_LENIENT` env-var probe | Demotes errors to warnings; not language-essential. REMOVE. |
| `runtime/desugar.rs` | 296 | `unify_world_syntax`, `unify_state_syntax`, `desugar_seq_concat`, `SystemBoundary` | `unify_world_syntax` is multi-FSM-only (rewrites `_world.X`/`world.X` to legacy `world`/`world_next` pair). `unify_state_syntax` is the terse-fsm-state desugaring — REMOVE with FSMs. `desugar_seq_concat` is a language feature (`++`) — that part should be KEEP, but move it out of this file. **Net REMOVE; salvage ~30 lines for `++`.** |
| `runtime/inject.rs` | 287 | `inject_claim_arg_types` + `inject_lhs_eq_types` (whole-program inference passes) | Type inference; useful but the user's framing is "language syntax + semantics + claim composition + Z3" — type inference is convenience, not core. Open question whether to KEEP or REWORK as `.ev`. Marked REMOVE as the default; see DEFER notes. |
| `runtime/validate.rs` | 21 | Wraps `portable::validate::enforce_external_only` + `register_subclaims` | The `enforce_external_only` half is external-FFI policy; with FFI rework, also goes. `register_subclaims` is 6 lines and is keep-worthy — move into `load.rs`. REMOVE. |
| `subscriptions.rs` | 73 | `AccessSets`, `body_references_identifier` for stdin-conflict detection | Multi-FSM scheduler input. REMOVE. |
| `event_sources/mod.rs` | 113 | `EventSource` trait, `WorldPluginCtx`, `WORLD_PLUGIN_INSTALLERS[7]` | Async event sources for multi-FSM scheduler. REMOVE. |
| `event_sources/file_line_reader.rs` | 159 | Async file-line reader → world writes | REMOVE. |
| `event_sources/stdin.rs` | 138 | Async stdin reader → world writes | REMOVE. |
| `event_sources/sigint.rs` | 130 | SIGINT → world write | REMOVE. |
| `event_sources/frame_timer.rs` | 122 | Tick timer thread → world writes | REMOVE. |
| `event_sources/file_watcher.rs` | 118 | Poll file mtime → world writes | REMOVE. |
| `event_sources/wall_clock.rs` | 111 | Wall-clock counter → world writes | REMOVE. |
| `event_sources/declarative_install.rs` | 101 | Generic FTI install driven by `install ∈ Seq(InstallStep)` | REMOVE. |
| `event_sources/reflection.rs` | 87 | Program-AST encoding plugin (world.program) | REMOVE. |
| `commands/common.rs` | 94 | Passthrough-desugar self-hosted pipeline (load passes from stdlib, query each user claim, rewrite) | Drives the auto-apply self-hosted desugar; with self-hosted passes gone, REMOVE. |

**REMOVE subtotal: ~ 5,650 lines across ~30 files.**

---

### REWORK — role survives, implementation should change

| Path | Lines | Current | What it should become |
|---|---:|---|---|
| `portable/mod.rs` | 409 | `EvidentRunner` + cached/guarded macros + runner pattern + `validate`/`subscriptions`/`toposort`/`seq_chains` sub-passes | The whole self-hosted-pass runner is sand-in-the-gears: every load runs Evident-walks-AST FSMs over the user's program for desugar/validate/inject/generics/subscriptions/toposort/seq_chains. With the scheduler removed, three of those passes (subscriptions/toposort/seq_chains) lose their consumer. Of the rest: a small Rust pass for `++` flattening and an even smaller one for "no FFI outside `external`" are cleaner than booting a sub-runtime. **REWORK as small Rust passes (~60 LOC each) or delete and let load fail loudly when an external pass would have applied. Most of this 409 disappears.** |
| `portable/generics.rs` | 313 | Generic monomorphization driven from `stdlib/passes/generics.ev` | Generics ARE a kept language feature. But the implementation should be a small Rust pass — the prior cutover gain was already null (per `MEMORY.md` REVIVE-generics). REWORK: rewrite as a ~150-line Rust pass that runs at load. |
| `portable/inject.rs` | 220 | `fsm_params` (FSM membership injection) + `prev_tick` (`_X` time-shift desugar) | Both passes are FSM-specific. With FSMs removed, the `_X` pattern goes too. REMOVE not REWORK. |
| `portable/desugar.rs` | 192 | `desugar_seq_concat` self-hosted (`++` flatten) | Rewrite as a ~30-line Rust AST pass. REWORK. |
| `portable/introspect.rs` | 146 | Schema-mutation rebuild via `stdlib/passes/introspect.ev` | Sole caller is `auto_apply_desugar` (REMOVE). REMOVE not REWORK. |
| `ffi.rs` | 384 | libffi/dlopen/dlsym call dispatch, HandleRegistry, sig parser (`i b s d f p v`) | The user says: "Rewrite as Evident FFIs." Concretely: the `libcall` primitive in CLAUDE.md is real and minimal. THIS file is the closest the existing runtime gets to that primitive but is much bigger because of HandleRegistry and the typed-resource conventions. REWORK down to the ~80-line libcall shape (CLAUDE.md's `src/ffi.py` lookalike). |
| `fti.rs` | 93 | FTI type-name → install fn registry (FrameClock, Timer) | Multi-FSM-coupled (installs event sources). Library code in Evident per CLAUDE.md. REMOVE alongside the event source layer. (Listed REWORK because the *concept* of "named C resource" survives in some form; the current Rust impl does not.) |
| `commands/effect_run.rs` | 74 | The one `effect-run` subcommand | The CLI itself stays, but `effect-run` is the multi-FSM driver. Replace with a single `evident <file.ev>` subcommand that loads + runs `query` on a top-level claim, or just dumps SMT-LIB. ~30 lines. REWORK. |
| `commands.rs` | 4 | Module decl | KEEP-shaped (will shrink to the one subcommand). |

**REWORK subtotal: current ~ 1,835 lines → target ~ 350–450 lines.**

(`portable/inject.rs`, `portable/introspect.rs`, `fti.rs` are listed REWORK above to flag they are not pure-KEEP, but their best fate is actually REMOVE; cumulatively they would otherwise add ~460 to REMOVE.)

---

### DEFER

| Path | Lines | Question for the user |
|---|---:|---|
| `runtime/inject.rs` | 287 | `inject_claim_arg_types` + `inject_lhs_eq_types` are the type-inference passes the language doc relies on heavily ("drop annotations the inference can recover"). Is this *type inference* in scope for the runtime, or should examples be expected to spell out types? If yes → KEEP; if no → REMOVE. Listed REMOVE above but flagging it here. |
| `translate/encode_ast.rs` and `translate/decode_ast.rs` | 944 + 854 = 1,798 | Most of the volume here is the AST-marshaler for the *self-hosted pass* runner (Evident's stdlib/ast.ev declares the AST datatype). With the self-hosted pass infra removed, do we still need any AST → `Value` round-trip? Or can these shrink to just "encode for the cached solver" (the smaller portion)? Likely answer: ~70% of volume can disappear. |
| `parser/patterns.rs` | 83 | `match` is a kept language feature — but `match` carries weight (pattern parsing + lowering). Quick KEEP unless `match` is also up for removal. |
| `translate/eval/extra.rs` | 267 | `evaluate_with_program_and_body` is purely a reflection-pass API. With reflection out, the canonical `evaluate` + a thin Datatype-pins variant cover everything. Question: do we want any reflection support at all, ever? If never → can simplify to ~100 LOC; if eventually → keep the seam. |
| Whether `runtime/load.rs`'s `imports`, file-walker, and `mark_system_loads_complete` survive | 145 | Imports stay; system-boundary is reflection-only and can go. Marking as DEFER because `load_file`'s import resolution loop touches both. Open question: which import patterns must keep working? |

---

## Concerns / responsibility catalog

| Group | Files | Lines | Dominant category | Notes |
|---|---:|---:|---|---|
| **Lexing** | `lexer.rs` | 360 | KEEP | Single file. |
| **Parsing & AST** | `parser/*` (8 files) + `core/ast.rs` + `core/value.rs` + `core/api.rs` + `core/mod.rs` + `core/seq_helpers.rs` + `core/z3_types.rs` | ~1,700 | KEEP | All KEEP. `core/z3_types.rs` partially shrinks if SetVar / DatatypeSetVar paths are unused. |
| **Translate / Z3 model construction** | `translate.rs` + `translate/{datatypes,declare,extract,preprocess}.rs` + `translate/eval/*` + `translate/exprs/*` + `translate/inline/*` | ~8,250 | KEEP | The bulk of the kept code. Internal cleanups but every file pulls its weight on a kept feature. |
| **Claim composition** | `translate/inline/*` (8 files) + `translate/exprs/mapping.rs` | ~1,300 | KEEP | This is responsibility (2). Stays. |
| **AST encode/decode for reflection / self-hosting** | `translate/encode_ast.rs` + `translate/decode_ast.rs` | 1,798 | KEEP / DEFER | These are KEEP because the cached-solver path uses parts of them to pin Datatype values. But ~60–70% is for the self-hosted pass runner and shrinks with it. |
| **Multi-FSM scheduling** | `effect_loop/*` (7 files) + `runtime/scheduler_api.rs` + `runtime/nested.rs` + `subscriptions.rs` | ~2,300 | REMOVE | The whole multi-FSM apparatus. User says go. |
| **Effect dispatch** | `effect_dispatch.rs` + `ffi.rs` + `fti.rs` | 1,116 | REMOVE / REWORK | `effect_dispatch.rs` REMOVE; `ffi.rs` REWORK down to a minimal libcall; `fti.rs` REMOVE. |
| **Async event sources** | `event_sources/*` (8 files) | 1,079 | REMOVE | Whole directory goes. |
| **Self-hosted pass runner** | `portable/*` (5 files) | 1,280 | REMOVE / REWORK | Some passes survive as direct Rust (REWORK), most disappear (REMOVE). |
| **Runtime state / load / passes** | `runtime/*` (10 files) | ~2,400 | mixed | `load.rs`/`mod.rs`/`query.rs`/`register_enums.rs` KEEP; `desugar.rs`/`inject.rs`/`reflection.rs`/`introspect.rs`/`nested.rs`/`autotune.rs`/`scheduler_api.rs`/`lenient.rs`/`validate.rs` REMOVE. |
| **CLI** | `main.rs` + `commands/*` (3 files) | 196 | REWORK (minimize) | Down to one subcommand. ~80 LOC total. |
| **Utility** | `z3_ctx.rs` + `stdlib_path.rs` + `lib.rs` + `commands.rs` | 194 | KEEP | All small load-bearing glue. |

---

## Projected final size

| Bucket | Current | Target |
|---|---:|---:|
| KEEP (after internal shrink in encode/decode/extra) | 11,300 | **~ 8,000** |
| REWORK (current → target) | 1,835 | **~ 400** |
| REMOVE (deleted entirely) | 5,650 | **0** |
| Glue/utility (already small) | ~120 | **~ 120** |
| **Total** | **18,666** | **~ 8,500** |

Predicted Rust runtime after this REMOVE pass plus modest internal
shrink in the AST marshaler: **~8.5K lines**. About half of the
current. To go further requires *also* shrinking the parser /
translator / inliner, which is changing the language, not the
runtime — out of scope for this audit.

The Python tiny-runtime in CLAUDE.md is < 1K lines because it
supports a **small** language (no `enum`, no `match`, no generics,
no FTI). Reaching < 500 lines in Rust requires the same
language-scope cut. This audit assumes the language scope stated in
CLAUDE.md is preserved.

---

## Top contentious calls

The three categorizations the user should sanity-check before deletions:

1. **`runtime/inject.rs` (287 LOC) — listed REMOVE, but it's the
   type-inference machinery.** The language documentation (CLAUDE.md
   §"Idiomatic Evident") heavily emphasizes dropped annotations
   recovered by inference. If `examples/` must keep working, this
   stays. If inference is a "nice-to-have we'll re-add later in
   Evident", it goes. **My call: REMOVE for the strict interpretation
   of the three responsibilities; KEEP if inference is part of
   "language semantics".** Marked DEFER above too — same question.

2. **`translate/encode_ast.rs` (944) + `translate/decode_ast.rs`
   (854) — listed KEEP, but ~60% is for the self-hosted pass
   runner.** Once `portable/*` is gone, large swathes of these
   become unreachable. I marked them KEEP because the cached-solver
   path uses `encode_ast::effect_results_to_value` and a few helpers.
   But the right call may be "shrink to ~300 LOC each" rather than
   blanket KEEP. **Verify by tracing what survives once `portable/`
   and `effect_dispatch.rs` and the scheduler are gone.**

3. **`ffi.rs` (384) — listed REWORK to a CLAUDE.md-shaped libcall
   primitive.** The CLAUDE.md story is that Evident-level FTI
   bridges call into a small libcall — but the existing `ffi.rs`
   has `StrArr` / `IntOut` / `I32Buf` / `PackedBuf` arg shapes that
   suggest non-trivial accumulated complexity for SDL/GL. **Question:
   is the CLAUDE.md "~80-line ffi.py" target real, or does GL/SDL
   need most of these shapes regardless?** If real → REWORK saves
   ~300 LOC. If not → REWORK is more like 100 LOC saved.

---

## Open questions

1. **Type inference (`runtime/inject.rs`):** is it part of "language
   semantics" (KEEP) or library convenience (REMOVE)?

2. **Generic types & `match`:** the per-file table assumes both
   stay. If either goes, `runtime/register_enums.rs`,
   `portable/generics.rs`, `translate/exprs/match_expr.rs`,
   `parser/patterns.rs` also REMOVE — saves ~1,000 more LOC.

3. **`run(F, init)` / `halts_within(F, N)`:** listed REMOVE. But the
   MEMORY entry "FSM constraint model (corrected)" says embedded FSMs
   are kept and now use a constraint surface `F(seed, fsm_state)`.
   Is *compile-time* FSM composition (the constraint surface) the
   right keep, with the *blocking-interpret* implementation REMOVED?
   That's what I assumed. Confirm.

4. **CLI:** does `evident effect-run <file>` survive (just driving a
   single top-level claim with no scheduler), or does it become
   `evident query <file> <claim>` or `evident dump-smtlib <file>`?
   The shape affects how `commands/effect_run.rs` rewrites.

5. **Will `examples/` be trimmed to a language-validation set, and
   if so, what's the trim list?** The decision drives which features
   the runtime must still support. If `examples/test_*_sdl_*.ev` go,
   the FFI rework target is much smaller.

6. **External claims policy:** `enforce_external_only` (~75 LOC
   across `runtime/validate.rs` + `portable/validate.rs`) rejects
   non-`external` schemas that construct FFI. With most FFI gone,
   does this validation rule survive in a tighter form, or just go?

7. **Subclaims:** `register_subclaims` (6 LOC in
   `runtime/validate.rs`) registers nested subclaim declarations as
   top-level schemas. This is the "subclaim" half of responsibility
   (2). It KEEPs; flag because the surrounding file is REMOVEd.

---

## Hard rule check

- No code deleted.
- No code written.
- Where the prior agent and the user disagreed (most of `portable/`
  / `effect_loop/` / `event_sources/` / `effect_dispatch.rs`),
  this report sides with the user: REMOVE is the default.
- Used the preloaded `/tmp/rust-dump.md` (18,046 lines, 94 files)
  as the primary source; read a small number of high-leverage files
  directly for verification.
