; prev.smt2 — previous-state pins for unsat_hello_done_to_init
; Pins: state = Done
; The transition relation forces state_next = Done for any input state.

(assert (= state Done))
(assert (= last_results (as seq.empty (Seq Result))))
