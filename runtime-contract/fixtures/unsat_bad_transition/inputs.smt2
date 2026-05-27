; inputs.smt2 — over-constraining pin for unsat_bad_transition
; The transition relation forces state_next = Count(2) when state = Format(3).
; Pinning state_next = Count(4) here contradicts that forced assignment => UNSAT.

(assert (= state_next (Count 4)))
