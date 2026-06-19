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
1. [x] **`query_cached` → build-once model.** Done (`cab77c4`). `CachedSchema` →
   `CompiledModel`, held directly by the executor (built once, eval per tick).
   Removed `query_cached`, `cache`/`cache_rebuilds`, the structural-signature
   rebuild logic, and the whole `EVIDENT_VALUE_CACHE` value cache. (`cached.rs`
   still holds `build_cache` — it's now the compiler, not a cache; rename later.)
2. [x] **Remove `lib_candidates` + fix package lib refs.** Done (`3912b2e`).
   `ffi_open` now `dlopen`s exactly the name the `LibCall` supplies; `packages/`
   bindings name Linux sonames directly (`libSDL2-2.0.so.0`, `libGL.so.1`,
   `libc.so.6`). SDL demos verified.
3. [x] **Remove ALL `EVIDENT_*` env-var-gated functionality + its code.** Done
   (`c6a02f9`). Zero `EVIDENT_*` left in `runtime/src`. Diagnostics deleted;
   toggles hardcoded (`FUNCTIONIZE` on, tactics fixed, arith-solver `2`,
   inline-depth `64`); `LENIENT` refactored env→threaded `bool` (deleted
   `lenient.rs`); deleted `timing.rs`; dropped the dead `--lenient`/`--arith-solver`
   /`--no-functionizer`/`--dispatch-timing` CLI flags. Original goals below:
   - **Diagnostics — delete the code entirely:** `JIT_TRACE`, `FUNCTIONIZE_TRACE`,
     `JIT_CALL_TRACE`, `JIT_DUMP`, `FZ_DUMP_BODY`, `INLINE_TRACE`, `FFI_TRACE`,
     `TRACE_SLOW_PATH`, `LOOP_TRACE`, `LOOP_TIMING`, `DISPATCH_TIMING`,
     `FUNCTIONIZE_STATS`, `DISPATCH_SEED` (+ all their `if env { eprintln! }` sites
     threaded through the functionizer / executor / dispatch).
   - **Config toggles — drop the env read, hardcode the default:** `FUNCTIONIZE`
     (always on), `TACTICS` / `EVIDENT_Z3_*` (default tactic chain), `LENIENT`
     (keep the functionizer fall-back mechanism, just un-gated), `MAX_INLINE_DEPTH`
     (fixed cap), `VALUE_CACHE` (goes with the `query_cached` refactor).
   - Also sweep magic numbers tied to removable features while in here.
4. [x] **Remove `runtime/examples/`.** Done — auto-discovered bench/probe
   binaries, no `Cargo.toml` entries needed removing (`9a91f48`).
5. [x] **Drop `check`, consolidate the CLI into one file.** Done (`684832e`).
   `commands/check.rs` + the check-only helpers (`load_runtime`,
   `split_files_and_flags`) removed; `common`/`effect_run`/`test` merged into one
   `src/commands.rs`; `commands/` dir gone. CLI: `test`, `effect-run`.
6. [x] **Remove `runtime/scripts/`.** Done — `cc-wrapper.sh` + `install-bin.sh`
   were referenced nowhere (`9a91f48`).
7. [x] **Audit `encode_ast.rs` / `decode_ast.rs`; rename or trim.** Done
   (`e6c4de9`). The dead AST-encode tree (gated by the caller-less reflection
   entry `evaluate_with_program_and_body`) was trimmed; the live Effect/Result
   codec merged into one `translate/effect_codec.rs` (`effect_encoder` /
   `effect_decoder`). Behavior-identical; SDL demos verified.
8. [x] **Review & collapse `effect_loop/` to a single FSM.** Done (`cab77c4`).
   `all_fsms() -> Vec<MainShape>` → `single_fsm() -> Result<MainShape>` (errors on
   0 or >1 FSMs); `scheduler.rs::run_loop` is now a flat single-FSM tick loop.
   **NOTE:** mario was genuinely 3 coordinating FSMs (game/keyboard/display over a
   shared `world`) — the only multi-FSM demo. Converted it to one `fsm main(world)`
   (input-poll + physics + render in one ordered tick), aligning with the
   "one main FSM, everything embedded" architecture. Still renders correctly.
9. [x] **Strip ALL comments** from `runtime/` Rust. Done (`41436fe`). A
   string/char/raw-string-aware stripper removed every `//`, `/* */`, `///`, `//!`
   across 103 files (−6389 lines); build clean + `./test.sh` green (267), so
   behavior-identical. Recoverable from git.

## Standing / owner action
- [ ] **Push** (no git creds in the container):
  `git push -u origin selfhost-main` (the 1037-commit archive of the old
  self-hosting `main`), then `git push --force origin main`.

## Deferred / optional
- [x] ~~Fold `runtime/lenient.rs` into `query.rs`~~ — done better by #3:
  `lenient.rs` deleted, the env side-channel replaced with a threaded `bool`,
  mechanism kept.
- [x] ~~Fully remove the inert `<T>` generics grammar + `SchemaDecl.type_params`~~
  — already done by an earlier pass (verified via Semfora): `try_parse_type_and_pins`
  has no angle-bracket handling, `type_params` exists nowhere in the code, and a
  regression test (`parse_generic_type_params_no_longer_accepted`) asserts
  `type Edge<T>` is rejected. The old "left inert" note was stale.
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
