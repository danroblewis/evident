# Plan — hybrid-coverage: drive the hybrid (Evident→SMT-LIB→greenfield engine) across the example corpus

**Mission.** hybrid-integration proved `runtime-smt fsm <file.ev>` runs 3 examples
byte-identical to `evident effect-run`. Extend that coverage toward running the
WHOLE `examples/test_*.ev` corpus end-to-end through the hybrid — the path to the
greenfield+transpiler being the actual go-forward runtime (split-plan's
recommendation). For each example it can't yet run, either close the
transpiler/engine gap or document why honestly.

## Orchestration protocol
Phase per feature-cluster of examples (parallel subagents within). For each
cluster: a subagent (a) runs each example via the hybrid + `evident effect-run`,
(b) for mismatches/failures, identifies the missing transpiler or engine
feature, (c) implements it (additive in `runtime-smt/`), (d) re-checks
byte-identical. Integrate, gate (`./test.sh` green + the cluster's examples
match), checkpoint, proceed.

## Phases (cluster by feature, order by leverage)
- P1 — scalar/Int/Bool/arithmetic FSMs (the simplest demos).
- P2 — enum-state + match FSMs.
- P3 — Seq / records.
- P4 — effects-heavy (the SDL/visual ones are likely out — FFI/GL; document).
- P5 — coverage report in `runtime-smt/COVERAGE.md`: which examples run hybrid
  end-to-end byte-identical, which don't + why (genuine gaps, e.g. SDL/GL FFI).
  Final `./test.sh` green ×2, push.

## Honest notes
- Some examples (SDL/GL visual) need FFI/GL the SMT engines won't reasonably
  cover — document as out-of-scope, don't force. The win is the FSM/logic corpus
  running through the hybrid, proving it's a real runtime, not 100% coverage.
- Additive; Evident path + `./test.sh` stay green. Push-only; don't merge.
