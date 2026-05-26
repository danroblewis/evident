# runtime/src/fsm_unroll/compose.rs — Z3-replaceability
**What it does:** Implements exponentiation-by-squaring FSM composition for `halts_within(F, N)` and closed-form `run(F, init)` (tier-1). Builds F^N via Z3 symbolic substitution (doubling + binary expansion), tracks cumulative halt and halted-state expressions, then asserts the resulting Bool into the outer solver or packages it as a `Z3Program` for the JIT.

**Criticality:** critical

**Verdict:** circular

**Confidence:** high

**How (if replaceable):** Not replaceable — this IS the mechanism that constructs the Z3 constraint for `halts_within`/`run`. It operates at the Z3 AST level (substitution, simplification) to build a closed-form constraint that the solver then checks. A "solve" cannot replace the thing that builds the constraint; that would be circular. The affine-gate check (delegating to `detector::classify`) prevents non-collapsing bodies from being unrolled, which is an engineering guard on the constraint-construction step itself, not a CSP.

**Change made:** none
