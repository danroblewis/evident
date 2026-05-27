# Plan — runtime-evolve: strategy 2, a WORKING SMT-LIB-driven mode of the existing runtime

**Mission.** Make the EXISTING `runtime/` accept **SMT-LIB + metadata** as input
(bypassing the Evident lexer/parser/translate), reusing its battle-shaped
EXECUTION ENGINE (`effect_loop/`, `translate/extract.rs`, `functionize/`,
`ffi.rs`, scheduler). The result: a working runtime that runs FSMs from SMT-LIB,
sharing the engine with the Evident-source path. This is "just update runtime/"
— strategy 2 (working runtime from the previous codebase), the counterpart to
`runtime-smt/` (strategy 1, from scratch).

**Constraint.** Additive/opt-in: the Evident-source path MUST keep working and
`./test.sh` MUST stay green. The SMT-LIB path is a NEW entry point, not a
replacement (yet). Read `docs/design/runtime-split.md` (split-plan's output) +
`runtime-contract/` (the oracle) when they land.

## Orchestration protocol
Phases in order; within a phase fan out the parallelizable components as PARALLEL
subagents (`general-purpose`, `sonnet`) in one message; wait; integrate; run the
gate; checkpoint-commit; proceed. Serial spine (wiring the SMT-LIB input into the
existing scheduler) is yours.

## Phase 1 — Find the seam (parallel survey)
Goal: identify exactly where SMT-LIB+metadata can enter the existing engine,
bypassing parse/translate. Subagents survey in parallel: (a) the scheduler/tick
entry (`effect_loop/`, what it needs — an FSM shape + per-tick solve), (b) the
model-extraction path (`translate/extract.rs` — can it extract from a model
solved from raw SMT-LIB?), (c) the Z3 context/solver setup (`runtime/` — how a
solver is built + how `given`/pins are asserted), (d) what the metadata must
declare (reconcile with `runtime-contract/FORMAT.md` if present).
**Gate:** a `docs/design/runtime-evolve-seam.md` you collate naming the entry
point + the minimal new code. Commit.

## Phase 2 — SMT-LIB input loader (the new front door)
Goal: load an SMT-LIB problem (`Z3_parse_smtlib2_string`) into the runtime's Z3
context + parse the metadata into the engine's FSM-shape struct (the same shape
`resolve_fsm` produces, but from metadata instead of an Evident AST). Subagents:
(a) the SMT-LIB→solver loader, (b) the metadata→FSM-shape builder, (c) wiring so
the scheduler accepts an FSM defined this way.
**Gate:** a single tick runs from an SMT-LIB+metadata fixture, model extracted,
matching the Evident-source path on the same FSM. Commit.

## Phase 3 — The loop, reusing the engine
Goal: full multi-tick run via the EXISTING scheduler/effect-loop — state
threading, effect dispatch, halt — driven by SMT-LIB. Subagents: (a) per-tick
state re-pin (assert prev model), (b) effect dispatch reuse, (c) halt. Most of
this is REUSING the engine; the new code is just feeding it from SMT-LIB.
**Gate:** a multi-tick fixture (countdown / a real demo) runs end-to-end via the
SMT-LIB path, matching the Evident-source path. Commit.

## Phase 4 — Multi-FSM + a real demo
Goal: ≥2 SMT-LIB-defined FSMs coordinated via the existing world plumbing; run a
real example (transpile one `examples/test_*.ev` to SMT-LIB+metadata via
`session-smtlib-frontend`'s transpiler, run it through this path, compare to
`evident effect-run`). **Gate:** the demo matches. Commit.

## Phase 5 — Validate + finalize
Run the `runtime-contract/` fixtures against this path (it should pass the same
oracle as `new-runtime`). Document the new SMT-LIB entry point + what was reused
vs new in `docs/design/runtime-evolve.md`. **Gate:** `./test.sh` green + contract
fixtures pass. Final commit + push `session-runtime-evolve`. **DO NOT merge to main.**

## Honest notes
- The WIN of strategy 2 is reusing the hard-won engine — so most phases are
  *wiring SMT-LIB into the existing engine*, not rebuilding it. If the engine is
  too entangled with the Evident-AST to feed from SMT-LIB cleanly (the split-plan
  may flag this), report exactly where, and reuse what you can — a partial
  working SMT-LIB path + an honest entanglement report is still strategy-2 progress.
- Keep it opt-in; the Evident-source path + `./test.sh` stay green throughout.
