# Plan — hybrid-integration (DISPATCHED 2026-05-26; preconditions met)

> **Status:** dispatched. The dispatch log + phase results are at the bottom
> (["Dispatch log"](#dispatch-log)). The original queued plan follows.


**Origin.** split-plan's decisive recommendation: HYBRID — greenfield the
orchestration engine, reuse the Evident→SMT-LIB transpiler front-end, port clean
subsystems, with the SMT-LIB+metadata interface as the keystone.

**Mission (when dispatched).** Wire the pieces into an end-to-end hybrid:
`Evident source → [transpiler] → SMT-LIB+metadata → [greenfield engine] → run`,
matching the current runtime on a real demo.

**Dispatch preconditions (gate before launching):**
1. `new-runtime` (`runtime-smt/`) engine reaches ≥N2 (a working tick loop that
   threads state + dispatches effects), passing `runtime-contract/` fixtures.
2. The Evident→SMT-LIB transpiler is landable (mature `session-smtlib-frontend`
   or new-runtime's own front-end phase) and emits the metadata the engine reads.
3. `runtime-contract/FORMAT.md` (the metadata convention) is settled.

**Phases (sketch — flesh out at dispatch with the real engine API):**
- P1: reconcile the transpiler's emitted metadata with the engine's expected
  metadata (one format). 
- P2: pipe transpiler output → engine; run one Evident FSM end-to-end via the
  hybrid; cross-check vs `evident effect-run`.
- P3: a real `examples/test_*.ev` end-to-end through the hybrid.
- P4: port any clean subsystem the engine still lacks (effects/FFI) from the
  legacy rather than rewriting.

**Note.** This is the convergence point of the two build tracks — once it works,
the greenfield engine + reused front-end IS the "next runtime," and the
strategy-1/strategy-2 comparison feeds the final architecture decision.

---

## Dispatch log

Preconditions verified met (2026-05-26):
1. `runtime-smt/` reaches N4 (tick loop + state threading + effect dispatch +
   multi-FSM + cache + scalar front-end), 155 tests green. ✓
2. Transpiler landable: `runtime-smt/src/frontend.rs` (scalar-claim) +
   `runtime/src/translate/smtlib.rs` (scalar QF). ✓
3. `runtime-contract/FORMAT.md` settled; 15 fixtures captured, the original
   `behavior_contract.rs` green 15/15 on both CurrentRuntime + pure-Z3 SmtLib. ✓

### Orchestration protocol

Phased; parallel subagents within a phase; a **gate** (compiles + runs + the
relevant test surface green) and a **checkpoint commit** end each phase. The
Evident-source path stays untouched and `./test.sh` stays green throughout —
every change is additive. No merge to `main`, no touching sibling branches.

### The keystone added deliverable — the dual-engine proof

`runtime-contract/` becomes a real Rust **lib crate** owning the engine-neutral
`FsmEngine` trait + fixture loader + matrix runner (`CVal`, `Outcome`,
`Verdict`, `run_matrix`). Both NEW engines implement that one trait and run ALL
15 fixtures, producing a **pass/fail matrix (fixture × engine)**:
  * **strategy 1** — the greenfield `runtime_smt::solve_tick` engine
    (`runtime-smt/tests/contract.rs`).
  * **strategy 2** — the existing runtime in SMT-LIB mode,
    `evident_runtime::smtlib_fsm::solve_tick` (`runtime/tests/contract_evolve.rs`,
    runs inside `./test.sh`).
Fixtures an engine genuinely can't satisfy = **documented `Gap`s, never faked**
(the only red verdict is `Fail` = a wrong answer). This proves both strategies
reproduce the captured semantics, and where each one's boundary lies.

### Phases (as executed)

- **P0** — create the shared `runtime-contract` lib crate (trait + loader +
  matrix); verify baselines (runtime-smt 155 green, runtime release built).
- **P1** — implement `FsmEngine` for both new engines; run all 15; capture the
  baseline matrix. Gate: both compile/run; `./test.sh` green.
- **P2** — convergence: Evident FSM source → SMT-LIB+metadata (front-end) →
  greenfield engine → run one real example end-to-end, byte-checked vs
  `evident effect-run`.
- **P3** — value-add (plan P4 "port a clean subsystem"): extend strategy 2's
  `solve_tick` to enum-state-via-SMT-LIB-datatypes (its documented next
  increment) so the enum-state `Gap`s become `Pass`es; re-run the matrix.
- **P4** — `runtime-contract/MATRIX.md` + a `run-matrix.sh` runner; finalize
  this log with the result.
