(declare-const s (Seq Int))
(assert (<= (seq.len s) 5))
(assert (= (seq.len s) 3))
(check-sat)
