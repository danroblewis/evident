; expected_model.smt2 — UNSAT fixture, no model exists.
;
; Fixture: unsat_bad_transition
; Cluster F (negative / UNSAT): problem ++ prev ++ inputs must be unsat.
;
; Rationale: state = Format(3) forces the transition relation to derive
; state_next = Count(3 - 1) = Count(2). inputs.smt2 asserts state_next = Count(4),
; which contradicts the uniquely-forced value. Hence unsatisfiable.
;
; Verify: cat problem.smt2 prev.smt2 inputs.smt2 > /tmp/u.smt2
;         echo "(check-sat)" >> /tmp/u.smt2
;         z3 /tmp/u.smt2   =>  unsat
