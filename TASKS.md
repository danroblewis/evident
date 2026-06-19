# TASKS

Work queue for the Evident runtime minimization. The refactors below mostly
touch `runtime/src/runtime/query.rs` + `runtime/src/translate/eval/` or the
parser, so they're done **sequentially** (one worktree agent, merged, before
the next) to avoid collisions. `./test.sh` (build + `cargo test`, SDL demos on
the Xvfb display) must be green after each.

## In progress
- [ ] **Remove 4 low-priority features** — worktree agent running
  - **Model sampling** — `rt.sample`, `runtime/sample.rs`, the blocking-clause
    loop in `translate/eval/mod.rs`, `tests/sample.rs` (no live callers).
  - **Toposort** — the dead `EVIDENT_TOPOSORT_IMPL=evident` dogfood path in
    `effect_loop/toposort.rs`; remove the Rust Kahn's effect-ordering too unless
    a demo needs dependency-ordered effects.
  - **UNSAT-core extraction** — `translate/eval/core.rs`, `query_with_core`, the
    wiring; rewire `commands/test.rs` to plain `query`.
  - **`external` FFI-boundary policy** — `validate.rs::enforce_external_only` +
    the gate checks, so any schema can call FFI.

## Queued (in order)
1. [ ] **`query_cached` → build-once model.** It caches the compiled Z3 *model*
   (`CachedSchema`), not results — and under one-FSM-as-one-solver there is only
   ever one model. Reshape so the runner builds the FSM model **once** at start,
   functionizes it, evaluates per tick, and drops it at halt. Remove
   `translate/eval/cached.rs`, `query_cached`, the `cache`/`cache_rebuilds`
   fields, `CachedSchema`-as-cache, and `EVIDENT_VALUE_CACHE`. The functionizer's
   compiled function is the only "cache" needed.
2. [ ] **Remove `lib_candidates` + fix package lib refs.**
   `ffi.rs::lib_candidates` hardcodes a macOS→Linux soname list; the runtime
   should just `dlopen` what the `LibCall` names. Delete it; update
   `packages/{sdl,gl,posix}/*.ev` to name the correct (Linux) library directly.
3. [ ] **Rip out trace/timing/dump scaffolding + magic-number sweep.** ~12
   diagnostic `EVIDENT_*` env gates (`JIT_TRACE`, `FUNCTIONIZE_TRACE`,
   `JIT_CALL_TRACE`, `JIT_DUMP`, `FZ_DUMP_BODY`, `INLINE_TRACE`, `FFI_TRACE`,
   `TRACE_SLOW_PATH`, `LOOP_TRACE`, `LOOP_TIMING`, `DISPATCH_TIMING`,
   `FUNCTIONIZE_STATS`) + their `if env { eprintln! }` code threaded through the
   functionizer/executor/dispatch. Also sweep magic numbers tied to removable
   features.
4. [ ] **Remove `runtime/examples/`.** The Rust bench/explore example binaries —
   not important. Delete the directory and drop any `[[example]]` entries in
   `runtime/Cargo.toml`.
5. [ ] **Drop `check`, consolidate the CLI into one file.** Remove
   `commands/check.rs` (the `check` subcommand + its `main.rs` dispatch) — don't
   care about it right now. Then move everything else (`effect_run.rs`, `test.rs`,
   `common.rs`) into a single `commands.rs` and delete the `commands/` directory,
   so there's exactly one file for the CLI. Remaining subcommands: `test`,
   `effect-run`.
6. [ ] **Strip ALL comments** from `runtime/` Rust (`//`, `/* */`, `///`, `//!`,
   including doc-comments and their doc-tests). Use a string/char/raw-string-aware
   stripper; build + full test must stay green (comments don't affect logic).
   Done **last**, so it also cleans up comments the earlier passes add.
   Recoverable from git.

## Standing / owner action
- [ ] **Push** (no git creds in the container):
  `git push -u origin selfhost-main` (the 1037-commit archive of the old
  self-hosting `main`), then `git push --force origin main`.

## Deferred / optional
- [ ] Fold `runtime/lenient.rs` (`LenientGuard`, ~30 lines) into `query.rs`;
  optionally drop the `--lenient` flag — but KEEP the mechanism (the functionizer
  relies on it to try-and-fall-back).
- [ ] Fully remove the inert `<T>` generics grammar + `SchemaDecl.type_params`
  from the parser/AST (left inert by the sweep; clean removal is fiddly because
  it's interwoven with `Seq(Edge<Rect>)` parsing).
- [ ] `runtime/stats.rs` (functionizer per-claim stats) — test-only after the
  profiling-flag removal; droppable with its tests.
- [ ] Build a NEW IDE for phase portraits / diagrams (the original goal). The viz
  prototypes + phase-portrait design docs live on the `diagrams-from-programs`
  branch; the old `ide/` is gone.

## Done this session (context)
- Reset `main` to the minimal Rust runtime (`f311f78`); archived the old
  self-hosting `main` → branch `selfhost-main` (1037 commits).
- Removed commands: query, sample, profile, desugar, infer-types, lint
  (kept: check / test / effect-run).
- Collapsed the executor: deleted `event_sources`, `subscriptions`, the
  multi-FSM wake scheduler, `Effect::SpawnFsm`.
- Reverted self-hosted desugar/inference/lint to inline Rust; deleted the
  reflection apparatus + `stdlib/passes/` + `stdlib/ast.ev`.
- Removed: generics + `toposort.ev`, experimental functionizers (llm/symbolic),
  the profile module, autotune, the decompose optimization, the profiling-flag
  suite, the conformance suite, `lints/`, `runtime-port/`, dead AST encode/decode.
- SDL/GL/posix FFI works cross-platform; demos render headless on Xvfb
  (Dockerfile.dev graphics stack + platform-aware `ffi_open`).
