; expected_model.smt2 — UNSAT fixture, no model exists.
;
; Fixture: unsat_hello_done_to_init
; Cluster F (negative / UNSAT): problem ++ prev ++ inputs must be unsat.
;
; Rationale: the transition relation asserts state_next = Done unconditionally
; (both Init and Done arms produce Done). inputs.smt2 asserts state_next = Init,
; which directly contradicts state_next = Done. Hence unsatisfiable.
;
; Verify: cat problem.smt2 prev.smt2 inputs.smt2 > /tmp/u.smt2
;         echo "(check-sat)" >> /tmp/u.smt2
;         z3 /tmp/u.smt2   =>  unsat
