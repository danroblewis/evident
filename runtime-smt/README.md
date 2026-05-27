# `runtime-smt/` — a greenfield SMT-LIB-input FSM execution engine

A clean, additive Rust runtime whose **input is SMT-LIB text + metadata**, not
Evident syntax. Z3 parses the SMT-LIB; this crate is the **execution engine**:
per-tick solve, state threading, effect dispatch, halt, multi-FSM scheduling.

It was built from scratch — unconstrained by the legacy `runtime/` — to discover
whether the SMT-LIB-text boundary yields cleaner structure than the legacy's
"build the Z3 AST through the C API and reuse contexts" design. It is **purely
additive**: it never touches `runtime/`, and `./test.sh` (the legacy test
suite) stays green.

See [`docs/plans/new-runtime.md`](../docs/plans/new-runtime.md) for the mission,
[`FORMAT.md`](FORMAT.md) for the fixture format.

## Build & run

Always build from the **repo root** — the repo-root `.cargo/config` supplies the
Z3 header path and the link-fixup wrapper:

```sh
cargo build --manifest-path runtime-smt/Cargo.toml
cargo test  --manifest-path runtime-smt/Cargo.toml          # 155 tests

# N0 floor — solve a raw SMT-LIB file, print the model.
cargo run --manifest-path runtime-smt/Cargo.toml -- solve runtime-smt/fixtures/n0_floor.smt2

# N2/N3 loop — run an FSM fixture to halt, dispatching effects to stdout.
cargo run --manifest-path runtime-smt/Cargo.toml -- run runtime-smt/fixtures/countdown.smt2
cargo run --manifest-path runtime-smt/Cargo.toml -- run runtime-smt/fixtures/two_fsms.smt2

# N4a transition cache — same loop, memoized; prints solve/hit stats to stderr.
cargo run --manifest-path runtime-smt/Cargo.toml -- run runtime-smt/fixtures/pingpong.smt2 --cache
#   [cache] 100000 ticks, 2 solves (99998 hits)

# N4b front-end — transpile a scalar Evident claim to SMT-LIB and solve it.
cargo run --manifest-path runtime-smt/Cargo.toml -- transpile runtime-smt/crosscheck/scalar.ev
```

Cross-check against the legacy runtime (the oracle):

```sh
cargo build --release --manifest-path runtime/Cargo.toml   # build the oracle once
runtime-smt/crosscheck.sh                                   # paired fixtures must agree
```

## Architecture

The crate is a pipeline of single-concern modules (≤ ~250 lines each). The flow
for one run:

```
fixture (.smt2 + embedded metadata)
  → meta.rs        load: split metadata (JSON) from per-FSM transition blocks → Problem
  → schedule.rs    order FSMs writer-before-reader (topological)
  → [per tick, per FSM, in order]
       world.rs       build the FSM's given inputs from shared world (this-tick writes win)
       assertion.rs   pin prev-state + given as SMT-LIB asserts
       z3c.rs         fresh Z3 context: parse transition + pins, check-sat, decode model
       model.rs       solved model → typed TickModel (next state, effects, halt, world writes)
       effect.rs      dispatch effects (Println → stdout, Exit → graceful halt)
       world.rs       fold the FSM's world writes back into the shared world
  → halt.rs        decide: Exit > halt-flag > no-progress
  → driver.rs / scheduler.rs   thread next-state → prev, world → next-tick world, repeat
```

| Module | Concern |
|---|---|
| `z3c.rs`       | **The Z3 floor.** RAII context (one per tick), solve, generic model decode (scalars + datatypes + sequences). The only module touching raw `z3-sys`. |
| `spec.rs`      | The frozen vocabulary: `Sort`, `Lit`, `StateVar`, `GivenVar`, `EffectSpec`, `HaltSpec`, `FsmSpec`, `WorldVar`, `Problem`; the typed result `TickModel` / `EffectValue`. Pure data + parsing. |
| `meta.rs`      | Load a self-contained fixture: embedded `; @meta`/`; @end` JSON + named `; @transition <fsm>` blocks → `Problem`. |
| `assertion.rs` | `Value` → re-injectable SMT-LIB; build the per-tick prev+given pin asserts. |
| `model.rs`     | Decode a solved model into a `TickModel` per the FSM's role assignments. |
| `tick.rs`      | `solve_tick`: compose assertion + Z3 + model into one tick (fresh context). |
| `effect.rs`    | Dispatch decoded effects to IO. `Println` → stdout, `Exit(code)` graceful. |
| `halt.rs`      | Pure halt decision. |
| `driver.rs`    | The single-FSM loop (`run_fsm`). |
| `schedule.rs`  | Writer-first FSM ordering (Kahn topo sort over shared-world deps). |
| `world.rs`     | Shared-world plumbing: init, build-given, record-writes. |
| `scheduler.rs` | The multi-FSM loop; `run` / `run_cached`. Subsumes the single-FSM case. |
| `cache.rs`     | `TickCache`: memoize tick solves by (FSM, prev, given). |
| `frontend.rs`  | A self-contained Evident-scalar-*claim* → SMT-LIB transpiler. |
| `fsm_frontend.rs` | The Evident-*fsm* → fixture transpiler (the convergence/coverage front-end): enum + scalar state, payload enums, `match`, `last_results`, intermediate vars, ternaries, `++`/string ops, multi-FSM + shared world. See `COVERAGE.md` for the example-corpus coverage it drives. |

## Milestones reached

| Milestone | What | Gate |
|---|---|---|
| **N0** | The Z3 floor: load SMT-LIB, solve, decode the model. | hardcoded `.smt2` solves; two-context isolation test |
| **N1** | The tick: pin prev + given, solve, decode next-state + effects. | countdown single-tick decrement; cross-checked vs `evident sample` |
| **N2** | The loop: thread state, dispatch effects (Println), halt. | `countdown.smt2` → tick/tick/tick/done + Exit(0); **byte-identical to the oracle** |
| **N3** | Multi-FSM scheduling over shared world (writer/reader). | `two_fsms.smt2` producer/consumer; byte-identical to the oracle |
| **N4a** | Transition cache. | `pingpong.smt2`: 100000 ticks → 2 Z3 solves |
| **N4b** | Evident-scalar → SMT-LIB front-end. | scalar claim transpiles + solves; sat verdict == oracle |
| **N5** | Evident-*fsm* front-end across the example corpus (`fsm_frontend.rs`). | **12** of `examples/test_*.ev` run end-to-end **byte-identical** to `evident effect-run` (scalars, nullary+payload enums, `match`, `last_results`, multi-FSM+world, constraint loops, string ops). Pinned by `tests/convergence.rs`; full ledger + honest boundaries in `COVERAGE.md`. |

Every fixture in `fixtures/` has a paired `.ev` in `crosscheck/` that the legacy
runtime executes; `crosscheck.sh` asserts the observable behavior (stdout + exit
code, or sat verdict) matches byte-for-byte.

## Test isolation by construction — the design that fixes the legacy's flakiness

The legacy runtime's test flakiness traced to **leaked `Z3_context`s and
`thread_local` solver/engine caches** that accumulated state across queries.
This crate is designed so that class of bug **cannot arise**:

- **One `Z3Ctx` per tick, freed on `Drop`.** `tick::solve_tick` creates a fresh
  `Z3Ctx`, solves, decodes, and lets the context drop at end of scope. The
  decoded `TickModel` is **owned Rust data** (`Value`, `String`, `i64`…) with no
  Z3 handles, so nothing escapes the context's lifetime.
- **No globals, no `thread_local`s, no `static` mutable caches** anywhere in the
  crate. An engine run is a value you create, use, and drop. Two runs — or two
  ticks — never share Z3 state. State threading happens entirely in Rust data
  structures (`BTreeMap<String, Value>`), not in a persisted solver.
- **The opt-in `TickCache` (N4a) preserves this.** It caches *decoded results*
  (owned `TickModel`s keyed by a string), never live Z3 handles — so the
  no-shared-Z3-state invariant holds even with caching on.
- **Errors are recoverable, not fatal.** `Z3Ctx::new` installs the NULL error
  handler, so a malformed SMT-LIB parse sets the error code (surfaced as a Rust
  `Err`) instead of aborting the process.

The contrast with the legacy is the central finding of this experiment — see the
comparison section below.

## What's cleaner than the legacy

### Isolation by construction (the headline finding)

The legacy runtime reaches for leaked Z3 contexts, `'static` sorts, thread-local
raw pointers, and per-pass engine caches — exactly the patterns that made its
tests flaky. Concrete, current citations:

| Legacy pattern | Where |
|---|---|
| `Box::leak(Box::new(Context::new(&cfg)))` — a Z3 `Context` leaked for a `'static` lifetime | `runtime/src/core/z3_program.rs:145` |
| `let leaked: &'static DatatypeSort<'static> = Box::leak(Box::new(dt));` — every user datatype sort leaked into a `'static` registry | `runtime/src/translate/datatypes.rs:144` |
| `thread_local!` holding a raw `*const EnumRegistry` + a `'static` datatype hint, set/cleared by an RAII guard | `runtime/src/translate/exprs/mod.rs:23,60` |
| `thread_local!` caching a full per-pass `EvidentRunner` (Z3 + Cranelift) for the thread's lifetime | `runtime/src/portable/mod.rs:67,89` |

`runtime-smt` has **zero** of these — no `Box::leak`, no `'static` Z3
lifetimes, no globals, no `thread_local`s. A `Z3Ctx` is a one-field struct whose
`Drop` calls `Z3_del_context`; `tick::solve_tick` builds one on the stack,
solves, decodes to owned Rust data, and drops it at end of scope. The test
`tick::tests::ticks_are_independent_no_state_leak` runs ticks out of order to
prove each is a pure function of its inputs. **The class of flakiness the legacy
works around cannot occur here.**

### Size / structure

Not apples-to-apples — `runtime-smt` is a from-scratch engine over a *smaller
input contract* (SMT-LIB text, not the Evident AST), and its line counts include
substantial inline tests and a scalar front-end. With that caveat:

| | Lines |
|---|---|
| **Legacy FSM-execution surface** (`effect_loop/*` + `subscriptions.rs` + `runtime/{inject,scheduler_api}.rs`) | ~2,310 |
| …which sits on top of `translate/` + `functionize/` + AST (the Evident→Z3 pipeline) | ~5,000+ more |
| **`runtime-smt` total** (incl. inline tests) | 5,141 |
|  — of which `frontend.rs` (scalar-**claim** transpiler) | 1,343 |
|  — execution kernel (everything else) | 3,798 |

The legacy loop is smaller in isolation, but it is inseparable from the ~5,000+
lines of translation/JIT beneath it. `runtime-smt` replaces that entire pipeline
with **one boundary — an SMT-LIB string** — and folds caching, multi-FSM
scheduling, world coordination, halt reasoning, effect dispatch, and metadata
loading into one cohesive ≤250-line-per-file crate. The cleanliness is
structural (one Z3 touch-point in `z3c.rs`; everything above sees `Value`), not
a raw line win.

### Split-vs-rewrite read

The SMT-LIB-input boundary is a credible cleaner foundation for the **execution**
half: the isolation design eliminates a whole class of bug, and the tick/scheduler
core is demonstrably simpler when the solver is a pure function of a string. But
the legacy's value is concentrated in what this engine does *not* cover — the
Evident→Z3 front-end, the FFI/effect system (SDL/GL/stdin), and the async
subscription scheduler — which carry hard-won semantics. The measured position:
**not a near-term rewrite.** The productive path is the one already in motion —
incrementally self-host bounded passes, treat this engine as the proof-of-concept
execution target, and grow the front-end until coverage is sufficient. The two
runtimes are complementary: the legacy front-end *feeds* this execution model.

## TODO / not yet covered

This is a minimal engine; completeness was explicitly not a goal. Honest gaps:

- **Effects beyond `Println` / `Exit`.** No FFI / `LibCall`, no FTI typed
  resources. `effect.rs` ignores unknown effect constructors rather than
  dispatching them. **`last_results` threading IS supported** (see
  `fixtures/feedback_loop.smt2`): the dispatcher maps each effect to a `Result`
  value (`IntToStr`→`StringResult`, `ParseInt`→`IntResult`/`ErrorResult`,
  `MonotonicTime`/`Time`→a deterministic `IntResult(0)` stub — no wall clock so
  runs stay reproducible), and the scheduler pins the prior tick's ordered
  `(Seq Result)` as the next tick's `last_results` given. No `.ev` cross-check
  pair for this yet — the greenfield `transpile_fsm` front-end doesn't transpile
  `match last_results[0]` / `IntToStr` FSM bodies (scalar subset only), so the
  fixture is hand-authored SMT-LIB and the threaded stdout is asserted by a Rust
  test (`scheduler::tests::feedback_loop_threads_last_results_across_ticks`).
- **No async event sources** (FrameTimer / Stdin / Sigint). The loop is purely
  subscription-free: all FSMs tick every tick. There is no blocking wait on
  external events.
- **Two front-ends, two scopes.** `frontend.rs` is the scalar-*claim* one-shot
  transpiler (Int/Nat/Pos/Bool/Real/String + arithmetic/Boolean/ternary/set-range
  membership). `fsm_frontend.rs` is the *fsm* transpiler that drives the corpus
  coverage (N5): enum (nullary + payload) and scalar state, `match` (with payload
  binding), `last_results` threading, intermediate Bool/String/Int body vars,
  ternaries, `++`/`index_of`/`substr`/`replace`, multi-FSM over a shared `world`,
  and per-tick constraint solves. What neither front-end yet does — records as
  FSM state, `Set`/recursive payload enums, inline `claim` composition, `_world`
  previous-world reads, and the embedded `run(F,init)`/`halts_within` execution
  tier — is itemized per example (feasible GAP vs genuine OUT) in `COVERAGE.md`.
- **Effect lists must be a Z3 `(Seq Effect)` or a cons-list datatype** in the
  fixture; the model decoder walks those shapes.
- **No per-tick external inputs** in the driver (givens come only from shared
  world in N3). Wiring stdin / a frame clock as a "given" source is future work.
- **Subscription-driven scheduling.** The legacy ticks an FSM only when its
  inputs change; here every FSM ticks every tick. For many-FSM programs a
  read-set-driven scheduler (as in `runtime/src/subscriptions.rs`) would matter.
- **Caching is in-memory only.** A `__pycache__`-style on-disk cache of decoded
  transitions (keyed by transition hash) is a natural extension and matches the
  project's AOT-cache direction.
