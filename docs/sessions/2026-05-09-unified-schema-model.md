# Session note — 2026-05-09: unified schema model

A long arc of work landing the "everything is a schema" model that
the project had been groping toward. Significant changes to runtime
behavior and substantial new docs. This file captures what shipped
so future readers don't have to git-spelunk.

## The framing that finally fit

After many iterations across multiple sessions, the answer for
"what IS an Evident model" landed as:

> A coordination unit with five things — read-set + write-set +
> private state + schedule + behavior. Implementation language is
> opaque (evident or Rust). The single coordination primitive is
> shared-state delta.

Captured in [`docs/design/schema-interface.md`](design/schema-interface.md).

## Implementation work (this session)

| Commit          | What                                                          |
|---|---|
| `7a136a3`       | Phase 1 — static read/write-set inference                     |
| `c2088aa`       | Phase 2 — delta scheduler (opt-in)                            |
| `ecc35dc`       | Phase 3 — drop fixpoint halt under delta                      |
| `0ef6020`       | Phase 4 v1 — single-FSM under delta (stdin works)             |
| `b3ea7dc`       | Flip default scheduler to delta                               |
| `9d043a1`       | Phase 4 v2 — graceful Effect::Exit                            |
| `fbc7a83`       | Lang test 05 — graceful shutdown demo                         |
| `fc51611`       | Phase 4 v3 — async event sources + FrameTimer                 |
| `4cd8885`       | Phase 4 v3.5 — per-FSM event matching + SIGINT                |
| `89af381`       | Multi-writer with disjoint fields                             |
| `f7fca86`       | Plugins-as-writers (FrameTimer/Signal/StdinSource)            |
| `31aea8c`       | Lang test 06 — echo using StdinSource                         |
| `918696a`       | Lang test 07 — timer demo                                     |
| `e72591d`       | Fix Z3 panic for payload-typed first state-variant            |
| `31a4ee8`       | Lang test 08 — word counter using payload state               |
| `17c2bcc`       | Reject ReadLine + StdinSource at load                         |
| `87e09e6`       | Lang test 09 — multi-plugin (timer + stdin)                   |
| `aae2bfd`       | Lang test 10 — SIGINT cleanup via plugin pattern              |
| `b9e4869`       | Demo — guess-the-number game                                  |
| `4cf23af`       | Integration test for lang test 09                             |
| `e3a9dba`       | Hoist disjoint-write check (single-FSM-with-plugin too)       |
| `b69e072`       | Startup trace under EVIDENT_LOOP_TRACE                        |
| `6a5a2c1`       | Lang test 11 — request/response coordination demo             |

## 2026-05-10 follow-up work

| Commit          | What                                                         |
|---|---|
| `17eb674`       | Effect::ParseInt + Effect::ParseReal + guess-number rewrite   |
| `ca10058`       | Rewrite effect_echo + effect_hello in modern style            |
| `912a64e`       | FileLineReader plugin (FTI v0 — file resource lifecycle)      |
| `cb1e6b4`       | Effect::SpawnFsm — dynamic FSM instantiation v1               |
| `c50e3db`       | WallClock plugin + lang test 12                               |
| `47b25e5`       | SpawnFsm takes Int arg pinned into spawned state              |
| `44821c3`       | FileWatcher plugin (poll-based mtime change detection)        |
| `066a813`       | Effect::ShellRun — synchronous shell command execution        |
| `6b1ffa4`       | Effect::IntToStr / RealToStr — symmetric to Parse*            |
| `aaf9247`       | spawnable_only marker — claims that don't auto-instantiate    |
| `833f419`       | FTI v1 — typed resource via parameter declaration             |
| `88a2b4b`       | FTI v1.5 — per-instance namespacing                           |
| `ed41dc9`       | Hostname FTI type + OneShotShellSource bridge                 |
| `20b984c`       | FTI v2 — per-instance configurable resources                  |
| `2654171`       | FTI subscription-aware wakes                                  |
| `34f5c94`       | SDL_Window FTI bridge — first real C-resource FTI             |
| `e3948c0`       | SDL_Window FTI extends to GL context + render demo            |
| `387951a`       | GL_Program FTI bridge — shader compile+link as declaration    |

## Documentation work

| Commit          | Doc                                                          |
|---|---|
| `05c3f96`       | `fsm-subscriptions.md` — initial design                       |
| `14c3e4c`       | `schema-interface.md` — the unifying framing                  |
| `68d575a`       | runtime-as-FSM + single-owner notes in subscriptions doc      |
| `a2fc4ce`       | `foreign-type-interface.md` + `fsm-spawning.md` directions    |
| `380f326`       | `multi-fsm-programs.md` cookbook guide                        |
| `003e8f8`       | Update banner on `multi-fsm.md`                               |
| Various         | CLAUDE.md updates, memory pointer updates                     |

## Capabilities now in the runtime

  * **Subscription-driven scheduling** (default; opt out via
    `EVIDENT_SCHEDULER=legacy`). FSMs tick only when world-delta /
    self-feedback / external event triggers fire.
  * **Multi-writer disjoint fields**. Multiple writer FSMs OK as
    long as their write-sets don't overlap.
  * **Plugin-as-writer**. FrameTimer / SigintSource / StdinSource
    auto-install when World declares reserved fields
    (`tick_count: Int`, `signal_received: Int`,
    `stdin_line: String`, optional `stdin_seq: Int`).
  * **Graceful Effect::Exit**. Same-tick effects from other FSMs
    complete before halting; exit code propagates to process exit.
  * **Single-owner enforcement**. ReadLine + StdinSource → load
    error. Plugin-owned fields participate in the disjoint check.
  * **Async event sources**. Background threads (FrameTimer,
    SigintSource, StdinSource) push to a wake channel; scheduler
    blocks on it when no FSM is otherwise ready.
  * **Per-FSM event subscription matching**. Marker types
    (`_ ∈ FrameTimer` etc.) coexist with the world-write path for
    back-compat.
  * **Payload-typed first state-variant** no longer crashes (Z3
    seeding fix).
  * **Pre-population of plugin-managed world fields** with type
    defaults so Z3 doesn't pick arbitrary values on tick 0.

## Worked examples

`programs/lang_tests/multi_fsm/`:
  * `01–04` — original multi-FSM tests
  * `05_graceful_shutdown.ev` — Exit + cleanup via world coordination
  * `06_echo.ev` — stdin reader with world-tracked seq counter
  * `07_timer_demo.ev` — counter waking on tick_count writes
  * `08_word_counter.ev` — same as 06 but using payload state
  * `09_timer_and_stdin.ev` — multi-plugin coordination
  * `10_sigint_cleanup.ev` — SIGINT-triggered cleanup
  * `11_request_response.ev` — two user FSMs coordinating via world

`programs/demos/`:
  * `effect_multi_fsm_transpiled.ev` — GL render with setup-then-halt
    pattern (the original perf motivation)

## Tests

  * **392 rust tests passing** under both default (delta) and legacy
    (`EVIDENT_SCHEDULER=legacy`) modes. Stability verified across
    multiple runs (16/16 scheduler tests, 9/9 multi-FSM tests
    all consistently pass). One test (request_response) skips
    under legacy mode because its gating semantics need
    delta-driven state-feedback scheduling.
  * **All multi-FSM lang tests** in `runtime/tests/multi_fsm.rs`
    (subprocess-based for those that need real stdin/SIGINT).
  * **Scheduler-specific tests** in `runtime/tests/scheduler_delta.rs`:
    multi-writer disjoint, multi-writer overlap rejection, payload-state,
    SIGINT, stdin conflict rejection, plugin auto-install, etc.
  * **Subscription inference tests** in
    `runtime/tests/subscriptions_demo.rs` — pin the read/write
    sets for the actual demo files.

## Design directions captured (no v1 implementation)

  * `docs/design/foreign-type-interface.md` — replace
    `Effect::FFICall` with declared types; bridge plugins
    materialize C resources. Auto-cleanup on scope exit, observable
    state, single-owner enforcement.
  * `docs/design/fsm-spawning.md` — dynamic FSM instantiation
    (per-connection servers, REPL evaluators, worker pools).

## What did NOT change

  * The constraint-solving substrate (Z3 + claim translation).
  * `Effect::FFICall` (still the FFI surface).
  * The core single-FSM step loop semantics.
  * Spec docs in `spec/` (unchanged; correct).
  * Existing FFI library code in `stdlib/{sdl,audio,shell}/`.

## Open questions / known limitations

  * **Effect-feedback loops**: an FSM that emits unconditionally
    will re-tick forever via effect-feedback. Documented in the
    multi-fsm-programs guide; users must gate.
  * **Drain-one-write-per-tick** can pile up unbounded if a plugin
    fires faster than ticks process. Acceptable for current
    rate-limited sources; bounded channel is future work.
  * **Marker-type subscriptions** still work (back-compat) but the
    plugin-as-writer path is now preferred for new code.
  * **Dynamic FSM instantiation** doesn't exist yet (see
    `fsm-spawning.md` for the design space).
  * **FFI-as-FTI** is documented as a direction; current FFI is
    still function-call-shaped via `Effect::FFICall`.

## How to extend

Adding a new plugin (e.g. `FileWatcher`):

  1. Define an `EventSource` impl in `runtime/src/event_sources.rs`.
     Background thread + write queue + `start()`/`stop()`/`drain_writes()`/`write_fields()`.
  2. Pick a reserved World field name; document.
  3. Wire auto-install into `effect_loop::run_with_ctx` —
     check for the field, install the plugin if present, mark
     plugin-owned fields.
  4. Test: a lang_test program that declares the field and
     observes the plugin's writes via subscription.

Adding a new built-in effect: existing `Effect::*` patterns in
`ast.rs` + `effect_dispatch.rs` + `decode_ast.rs` remain the same.

## Where to read first (for next session / new contributor)

  1. [`docs/design/schema-interface.md`](design/schema-interface.md)
     — the model.
  2. [`docs/guide/multi-fsm-programs.md`](guide/multi-fsm-programs.md)
     — the cookbook.
  3. [`docs/design/fsm-subscriptions.md`](design/fsm-subscriptions.md)
     — the scheduler.
  4. [`CLAUDE.md`](../CLAUDE.md) — project guide.
