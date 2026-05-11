# Wave 1+2 findings summary

34 code-review subagents ran in parallel against every file in
`runtime/src/`. Each wrote per-file findings in this directory; this
is the cross-cutting view.

## Files reviewed: status

  * **22 clean** — no rule violations, no invariant violations.
  * **12 with violations** — see "Violations of existing rules / invariants" below.

Clean: lexer, ast (modulo documented AP-001 exemptions), pretty,
translate-mod, translate-types, translate-datatypes, translate-extract,
translate-decode_ast (modulo exemptions), subscriptions, ffi (modulo
exemptions), fti, effect_dispatch (modulo exemptions),
event_sources (8 of 9 bridges), commands-mod, commands-check,
commands-query, commands-sample, commands-test, commands-effect_run,
commands-lint, parser (modulo "no source position in errors"),
main (modulo dispatch-table gap).

With violations: translate-mod (re-export widening), translate-declare
(asserts constraints), translate-preprocess (builds Z3 expressions),
translate-exprs (cycle), translate-inline (knows about scheduler
marker), translate-eval (no section markers, scattered imports),
translate-encode_ast (cross-language drift), runtime
(execution-layer scaffolding), effect_loop (hard-codes specific
bridges), commands-common (single-use helpers), commands-desugar
(missing CLI half), commands-infer_types (missing CLI half + rule
special-casing), lib (unjustified pub mods), event_sources
(SdlWindowSource carries GL concerns).

## AP-NNN numbering collision

Multiple agents independently picked AP-009/010/011 for different
new candidate rules. The next available number is AP-009. The
candidates from across the wave need to be deduped, ranked, and
renumbered before any get promoted to `lints/rules/`. The "Recurring
patterns" section below clusters them.

## Recurring patterns (strong candidates for new rules)

These appeared in 2+ files independently, so they're not one-offs.

### Pattern A: dual-role files missing their CLI half

Both `commands/desugar.rs` and `commands/infer_types.rs` were
documented as "dual role" (CLI verb + library hook), but each
ships only the library half — no `cmd_desugar` / `cmd_infer_types`
function exists, and `main.rs`'s dispatch table has no arm for
either even though its doc-comment lists `infer-types`.

Either: add the missing `cmd_*` functions and dispatch arms, OR
update the runtime-invariants doc to declassify these from "dual
role" to "library-only" and move them out of `commands/`.

Mechanizable as a grep: every file in `commands/` (except
`common.rs` and `mod`) must declare `pub fn cmd_<name>` AND that
name must appear in `main.rs`'s match block.

### Pattern B: scheduler / runtime knows about specific bridges

Three files violate the "must not know about specific FTI types
or bridge structs" family of invariants:

  * `effect_loop.rs` lines 305-465: hard-coded auto-install blocks
    for FrameTimer, SigintSource, StdinSource, WallClockSource,
    FileWatcherSource, FileLineReader, plus references to
    `crate::fti::is_fti_type` / `fti_install_fn` / `FtiContext`.
  * `runtime.rs` lines 486-490: `STDLIB_SHIMS` const hardcodes
    `"stdlib/sdl.ev"` directly in the language-core facade.
  * `runtime.rs` 5 of 31 public methods (`query_with_pinned_datatypes`,
    `query_with_pins_and_given`, `enums_registry`, `z3_context`,
    `encode_effect_result_list`) exist solely as scaffolding for
    the effect loop; their doc comments name "effect loop" /
    "multi-FSM scheduler" as the calling context.

The fix shape is the same in each case: introduce a registry
(WORLD_PLUGIN_INSTALLERS mirroring fti::INSTALLERS), or move
the execution-layer-coupled methods into a separate trait /
extension type, or load shim paths from a config file.

### Pattern C: cross-cmd duplication that should live in common.rs

Three pairs found:

  * `query.rs` and `sample.rs` duplicate a ~25-line load + flag-
    parse + desugar/infer-pipeline prologue.
  * `query.rs` and `sample.rs` both hand-strip `--strict` from
    `args` instead of declaring it on `common::Flags`.
  * `lint.rs`, `desugar.rs`, `infer_types.rs` each declare a
    private `STDLIB_AST = "stdlib/ast.ev"` const and repeat the
    same load-AST + load-pass + mark-system + load-user sequence.

The invariant says single-use helpers stay in their command file
and shared helpers go in `common.rs`. The converse is implicit:
duplicated helpers must move to `common.rs`. Worth making
explicit as a rule.

### Pattern D: single-use "shared" helpers in common.rs

The inverse of pattern C: `common.rs` exports `print_query_result`
(only used by `query.rs`) and `value_as_json` (only used by
`sample.rs`). Each violates the "single-use helpers stay in their
command file" invariant.

### Pattern E: lib.rs publishes more than necessary

6 of 11 `pub mod` declarations (`ffi`, `lexer`, `parser`,
`event_sources`, `fti`, `runtime`) have no `evident_runtime::<name>`
caller in `runtime/tests/` or `runtime/src/commands/`. The
runtime ones especially: `runtime` is redundant because the
`pub use runtime::{EvidentRuntime, QueryResult, Value}` line
already publishes the canonical facade.

### Pattern F: stale documentation drift

Multiple cases of doc-comments referencing things that have moved
or no longer exist:

  * `runtime.rs` doc-comments reference `cmd_execute`,
    `executor::load_io_stdlib`, `plugins::sdl::STDLIB_SDL_EV` —
    all from the deleted Python runtime.
  * `translate.rs` doc-comment points to a non-existent
    `runtime/PROGRESS.md`.
  * `desugar.rs` doc carries a `translate/inline.rs:223` line
    reference that's almost certainly stale.
  * `main.rs` doc-comment lists `infer-types` but no dispatch
    arm.
  * `eval.rs` module doc claims "four entry points" but there
    are nine.

## Single-file invariant violations (not yet rules; review-and-fix)

  * **runtime.rs**: 5 facade methods are execution-layer
    scaffolding (the `query_with_pinned_*` family + `z3_context`
    + `encode_effect_result_list`). Either move to a separate
    trait / extension type or document the exception.
  * **effect_loop.rs**: hard-coded bridge install blocks (see
    pattern B). Plus `unsafe { mem::transmute }` at lines 129-131
    with no SAFETY comment. Plus ~6 `std::env::var` calls in
    per-FSM hot loops.
  * **declare.rs**: 5 `solver.assert(...)` sites (lines 93, 98,
    131, 148, 171) for Nat / Pos / Seq-length non-negativity.
    Invariant says declaration must NOT assert constraints.
    Should move these to `inline.rs`.
  * **preprocess.rs**: `apply_seq_lengths` (315) and `literal_range`
    (353) take `&z3::Context` and build expressions. Invariant
    says preprocess must NOT build Z3 expressions.
  * **exprs.rs**: confirmed cycle with preprocess (line 13 imports
    `env_clone` and `literal_range`). Invariant forbids the cycle;
    fix is to move the helpers to `types.rs`. Also: at 1863 lines
    with 9 identifiable concern groups and only 1 `// ──` section
    header, file is undersectioned.
  * **inline.rs**: line 507 knows about `spawnable_only`
    scheduler-side body marker. Invariant says inline must NOT
    know about scheduler.
  * **eval.rs**: imports split across lines 5-7 and 103-108 (the
    "scattered imports" the invariant explicitly forbids). Zero
    `// ──` section markers despite invariant requiring them.
    `populate_enum_variants` (75-101) is a section-2/3 helper
    sitting in the section-1 region.
  * **translate.rs (mod)**: `pub use` list at lines 40-46
    widened beyond the documented allow-list with 6 additional
    items. Plus 3 `pub mod foo { pub use ... }` wrapper modules
    (`ast_decoder`, `ast_encoder`, `preprocess_api`) routing
    around the allow-list — `commands/test.rs:18` already uses
    one as a back door.
  * **encode_ast.rs**: silently drops `SchemaDecl::param_count`
    (no slot in stdlib/ast.ev's `MakeSchemaDecl`) — breaks
    interface-vs-helper-locals semantics on round-trip. Silently
    coerces `EffectResult::Real` to `NoResult` despite
    `RealResult(Real)` existing in stdlib/runtime.ev. Also: the
    encoder mirrors `Result`/`ResultList` from stdlib/runtime.ev
    in addition to stdlib/ast.ev, so the cross-language contract
    is wider than the invariant says.
  * **SdlWindowSource (in event_sources.rs)**: dlopens
    OpenGL.framework and calls glGenVertexArrays /
    glBindVertexArray / glViewport itself — GL-bridge concerns
    living in the SDL bridge. Latent cross-bridge violation that
    will surface once event_sources.rs is split.

## What this DOESN'T tell us

  * Anything about `examples/`, `stdlib/`, or `tests/` — only
    `runtime/src/` was reviewed in this wave.
  * Whether the runtime-invariants doc itself is right; agents
    treated it as ground truth.
  * Whether mechanically-clean files have semantic problems;
    agents only checked against rules + invariants, not "is this
    code correct."

## Suggested next steps

In priority order, with rough effort estimates:

  1. **Fix the patterns that block real work** (high value, low
     effort): pattern A (add missing cmd_* functions and dispatch
     arms) + pattern F (clean up stale doc references). Maybe
     1 hour of mechanical edits.

  2. **Promote the strong pattern candidates to rules** (high
     value, medium effort): pattern A → AP-009; pattern C → AP-010;
     pattern D → AP-011; pattern E → AP-012. Each gets a rule file
     in `lints/rules/` + a check in `lints/checks.sh`. Probably
     1-2 hours.

  3. **Move declare.rs's solver.assert calls into inline.rs** so
     declare's invariant holds. Single-file refactor, an hour.

  4. **Move preprocess.rs's Z3 expression building into a different
     module** (or remove it if it's unnecessary) so preprocess's
     invariant holds. Same scope.

  5. **Break the exprs ↔ preprocess cycle** by moving env_clone +
     literal_range into types.rs. Single-file refactor with
     compile-time verification.

  6. **Remove the scheduler-knowledge leak in inline.rs**
     (line 507's `spawnable_only` reference). Move whatever logic
     uses it into effect_loop.rs.

  7. **Pattern B is the biggest structural fix** (medium value,
     high effort): introduce WORLD_PLUGIN_INSTALLERS in
     event_sources/, refactor effect_loop's hard-coded blocks to
     read from it, and figure out what to do with runtime.rs's
     5 effect-loop scaffolding methods. This is the "Scheduler
     trait" the user originally asked about — the agents
     independently arrived at the same conclusion. Probably a
     half-day to a day.

  8. **Split event_sources.rs into per-bridge files** + extract
     `event_sources/mod.rs` for the trait + helpers. Then the
     SdlWindowSource GL contamination becomes a real lint
     violation. Half a day.

  9. **Tighten lib.rs's pub mod list** per pattern E. Demote
     `ffi`, `lexer`, `parser`, `event_sources`, `fti`, `runtime`
     to `mod`. Verify nothing breaks. ~30 minutes.
