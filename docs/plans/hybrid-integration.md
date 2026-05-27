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

### Results (all phases landed; `./test.sh` green throughout)

**The dual-engine matrix** (`runtime-contract/MATRIX.md`, regenerate with
`runtime-contract/run-matrix.sh`). Verdicts: `✓` full (state+effects), `✓ˢ`
state-only (effects not in the portable SMT for that fixture — `effects_in_smt:
false`), `—` documented gap, `✗` wrong answer.

| Engine | ✓ | ✓ˢ | — | ✗ |
|---|---|---|---|---|
| CurrentRuntime (gate) | 15 | 0 | 0 | 0 |
| SmtLib (pure Z3, Method A/B/UNSAT) | 15 | 0 | 0 | 0 |
| **Strategy 1 — greenfield** (`runtime-smt`) | 10 | 5 | 0 | **0** |
| **Strategy 2 — existing, SMT-LIB v1 scalar** | 2 | 1 | 12 | **0** |
| **Strategy 2 — existing, enum-increment** | 10 | 5 | 0 | **0** |

- **Strategy 1 reproduces all 15** captured ticks (the 5 `✓ˢ` are the
  `effects_in_smt:false` positives whose effects the capture leaves to the
  runtime engine; the 2 negatives are witnessed as genuine UNSAT).
- **Strategy 2 shows the increment explicitly.** v1's 12 `—` are all one
  documented boundary — enum-typed `state` via SMT-LIB `(declare-datatypes …)`.
  P3 crossed it with **one additive function**, `smtlib_fsm::solve_smtlib_decode_all`
  (generic raw-`z3-sys` model decode; no registered `DatatypeSort` needed), after
  which strategy 2 **matches strategy 1 exactly (10 ✓ / 5 ✓ˢ / 0 ✗)**.
- Net: **both split strategies reproduce the captured semantics by independent
  code paths**, and where each one stops is documented, not faked.

**Convergence** (P2). The full pipeline `Evident FSM → transpile_fsm →
SMT-LIB+metadata → greenfield engine → run` is **byte-identical** to
`evident effect-run` on `runtime-smt/crosscheck/countdown.ev` and the REAL
examples `examples/test_08_exit_code.ev` (exit 42) + `examples/test_03_seq_chain.ev`
(`runtime-smt/tests/convergence.rs` + `crosscheck.sh`).

**What this feeds the architecture decision.** Engine 3 (greenfield) is the
cleaner *execution* foundation (isolation-by-construction; one SMT-LIB-string
boundary). Engine 5 proves the *existing* runtime can be evolved to the same
behavior additively. The split-vs-rewrite read from `runtime-smt/README.md`
holds: the productive path is the greenfield engine as the execution target +
the legacy front-end feeding it — and `transpile_fsm` is the first real strand
of that front-end. The remaining honest gaps (async event sources, `last_results`
threading, FFI effects, mode-2 dispatch) are the next front-end/engine increments,
recorded — none faked.

### Files (additive; Evident-source path + the original behavior_contract.rs untouched)

- `runtime-contract/` → a lib crate: `src/{lib,value,fixture,engine}.rs`
  (`FsmEngine` + `CVal` + loader + matrix runner), `MATRIX.md`, `run-matrix.sh`.
- `runtime/tests/contract_evolve.rs` (strategy 2, both columns; runs in `./test.sh`).
- `runtime/src/smtlib_fsm/decode.rs` + `mod.rs` re-export (the enum increment).
- `runtime-smt/tests/contract.rs` (strategy 1), `runtime-smt/src/fsm_frontend.rs`
  + `main.rs fsm` subcommand + `tests/convergence.rs` + `crosscheck.sh` (convergence).
