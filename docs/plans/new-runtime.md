# Plan — new-runtime: a greenfield Rust SMT-LIB-input FSM engine

**Mission.** A clean Rust runtime whose INPUT is SMT-LIB (+ metadata), not
Evident syntax — Z3 parses the SMT-LIB; this runtime is the EXECUTION ENGINE
(per-tick solve, state threading, effect dispatch, halt, scheduling). Built via
`rust+Z3 → rust+SMT-LIB`, unconstrained by the legacy, to discover cleaner
structure. Validated against `runtime-contract/` (the oracle) + cross-checked
vs the current runtime.

**Deliverable dir:** `runtime-smt/` (own crate or workspace member). Additive —
never touch the current `runtime/`; keep `./test.sh` green.

## Orchestration protocol
Execute phases in order. Within a phase, fan out the parallelizable COMPONENTS
as subagents (`general-purpose`, `sonnet`) in one message (concurrent); wait;
integrate yourself; run the phase gate (build + the milestone's cross-check);
commit a checkpoint; proceed. The serial spine (assembling components into the
loop) is yours; the separable components are subagent work.

## Phase 0 — Scaffold + N0 (Z3 floor)
Set up `runtime-smt/` (Cargo crate, link z3 / z3-sys), and N0: load an SMT-LIB
string, solve via Z3, extract the model. (≈ raw `z3` — the floor everything
builds on.) **Gate:** `cargo build` + a test solving a hardcoded `.smt2`. Commit.

## Phase 1 — N1: the TICK (parallel components)
The tick is what makes this not-just-Z3. Subagents build in parallel:
  - **metadata parser** — read the `runtime-contract/FORMAT.md` metadata (if
    present, else design minimal): which vars are state/state_next/effects/given.
  - **assertion builder** — assert prev-state values + given inputs onto the
    solver alongside the transition constraint.
  - **model extractor** — after check-sat, read next-state vars + effect vars
    from the model into a typed result.
You assemble: `tick(problem, meta, prev, inputs) -> (next_state, effects)`.
**Gate:** the tick reproduces a `runtime-contract/` single-tick fixture (or, if
the contract isn't landed yet, cross-check one decrement-style tick vs the
current runtime). Commit.

## Phase 2 — N2: the LOOP
State threading (next_state → prev for the next tick), effect dispatch (start
with Println via real IO), halt (a halt signal / no-progress). Subagents:
(a) the driver loop, (b) the effect dispatcher (Println first), (c) the halt
detector. **Gate:** a multi-tick fixture (e.g. countdown to halt) runs end-to-end,
matching the contract / current runtime. Commit.

## Phase 3 — N3: multi-FSM scheduling
≥2 FSMs sharing world state, coordinated (writer/reader). Subagents:
(a) the scheduler (which FSM ticks when — start simple: round-robin or
all-each-tick), (b) shared-world read/write plumbing. **Gate:** a 2-FSM
contract fixture passes. Commit.

## Phase 4 — N4 (stretch, pick by remaining time)
Either: (a) JIT/cache a hot transition (avoid re-solving identical inputs), or
(b) a thin Evident→SMT-LIB front-end (reuse `session-smtlib-frontend`'s
transpiler work). **Gate:** the chosen stretch demonstrably works on a fixture.
Commit.

## Phase 5 — Document + finalize
`runtime-smt/README.md`: architecture, milestones reached, **what's cleaner
than the legacy** (concrete — this feeds the split-vs-rewrite decision), the
test-isolation design (no leaked-Context-per-engine accumulation — avoid the
legacy's fragility), and TODO. **Gate:** `cargo build` + all reached fixtures
pass + `./test.sh` green (current runtime untouched). Final commit + push
`session-new-runtime`. **DO NOT merge to main.**

## Honest notes
- Land N0–N2 solidly (a real tick loop threading state + dispatching an effect);
  N3/N4 as time allows. A working minimal engine cross-checked vs the oracle is
  the win; completeness is not expected.
- Design for test isolation from line one (the legacy's leaked Z3 contexts +
  thread_local engines are exactly what made it flaky — don't reproduce that).
- Borrow architecture from the C++ `runtime-c/` (it reached M5) where useful.
