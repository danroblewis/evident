# Plan — hybrid-integration (QUEUED; dispatch when inputs ready)

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
