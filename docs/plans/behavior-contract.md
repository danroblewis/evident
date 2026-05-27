# Plan — behavior-contract: a portable FSM-engine behavior oracle

**Mission.** Capture the current runtime's hard-won execution semantics as
**implementation-agnostic fixtures** (SMT-LIB constraint + metadata + prev-state
+ inputs → golden next-state-model + effects, models serialized as SMT-LIB
text), with a pluggable engine trait. This preserves the *behavior* while the
*code* is free to be rewritten, and is the oracle `new-runtime` and any split
engine must pass.

**Deliverable dir:** `runtime-contract/`. Additive — never restructure `runtime/src`.

## Orchestration protocol (the session is an ORCHESTRATOR)
Execute the phases below IN ORDER. Within a phase, **fan out the listed work
items as parallel subagents** (`general-purpose`, model `sonnet`) in a single
message so they run concurrently; wait; **collate** their outputs yourself;
run the phase **gate**; **commit a checkpoint**; proceed. Keep subagents tight
(focused reads + a structured artifact, no essays). Do not start a phase until
the prior phase's gate passes.

## Phase 1 — Behavior survey (parallel)
Goal: a catalog of every distinct engine behavior worth pinning.
Subagents (1 per area, ~7 parallel), each reads `runtime/src/effect_loop/` +
runs the relevant `examples/test_*.ev` and writes `runtime-contract/survey/<area>.md`
describing the observable contract of its area:
  - tick (one transition: prev-state+inputs → next-state)
  - state threading across ticks (`_var`/state pair)
  - effect emission + ordering
  - halt (implicit halt, `Effect::Exit`)
  - multi-FSM coordination via shared world
  - `given`-pinned inputs
  - last_results / effect feedback
**Gate:** a `runtime-contract/survey/INDEX.md` you collate listing the
behaviors to capture as fixtures. Commit.

## Phase 2 — Metadata/convention format (focused)
Goal: define how a flat SMT-LIB problem declares FSM structure (which vars are
state / state_next / effects / given / halt). 1–2 subagents draft options
(naming convention vs `(set-info)` annotations vs sidecar JSON); you pick one.
**Gate:** `runtime-contract/FORMAT.md` — the metadata spec. Commit.

## Phase 3 — Fixture capture (parallel, the bulk)
Goal: for each behavior in Phase 1's INDEX, a fixture = `(transition SMT-LIB +
metadata + prev-state + inputs) → golden (next-state model as SMT-LIB text,
effects)`. Subagents (1 per behavior cluster, parallel) each: build the SMT-LIB
(transpile a small Evident FSM via `runtime/src/translate/smtlib.rs` where the
subset covers it, else hand-write SMT-LIB+metadata — note which), run the
CURRENT engine to derive the golden output, and write
`runtime-contract/fixtures/<name>/{problem.smt2, meta.json, prev.smt2,
inputs.smt2, expected_model.smt2, expected_effects.txt}`.
**Gate:** ≥6 fixtures across the core behaviors exist + are self-consistent. Commit.

## Phase 4 — Harness + current-runtime adapter
Goal: prove the fixtures match current behavior, via a pluggable interface.
Subagents in parallel: (a) define `trait FsmEngine { fn tick(problem, meta,
prev, inputs) -> (model, effects) }`; (b) implement a `CurrentRuntimeEngine`
adapter over `EvidentRuntime`; (c) a fixture runner that loads each fixture,
runs an engine, and diffs vs golden. You integrate.
**Gate:** the runner passes ALL fixtures against `CurrentRuntimeEngine`
(proves the capture is faithful). `./test.sh` green. Commit.

## Phase 5 — Document + finalize
`runtime-contract/README.md`: the format, how to add a fixture, how a NEW
engine plugs in (implement `FsmEngine`, run the suite). Note "contract TODOs"
for features the current runtime lacks (design fresh, don't fake). 
**Gate:** `./test.sh` green ×1. Final commit + push `session-behavior-contract`.
**DO NOT merge to main.**

## Honest notes
- Depth on the CORE behaviors beats broad-but-shaky. The fixtures are the
  asset; the adapter proves them.
- If `translate/smtlib.rs`'s subset can't express a behavior's SMT-LIB,
  hand-write it + note the gap (that gap is a transpiler TODO, not a blocker).
