; inputs.smt2 — over-constraining pin for unsat_hello_done_to_init
; The transition relation forces state_next = Done for any state (both arms => Done).
; Pinning state_next = Init here contradicts that forced assignment => UNSAT.

(assert (= state_next Init))
