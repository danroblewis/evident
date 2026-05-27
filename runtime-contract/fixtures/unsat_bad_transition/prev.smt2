; prev.smt2 — previous-state pins for unsat_bad_transition
; Pins: state = Format(3)
; Format(3) forces the transition relation to produce state_next = Count(2).

(assert (= state (Format 3)))
(assert (= last_results (as seq.empty (Seq Result))))
