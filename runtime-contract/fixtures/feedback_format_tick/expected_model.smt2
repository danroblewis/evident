; feedback_format_tick — golden next-state witness
; Captured from: evident test runtime-contract/fixtures/feedback_format_tick/source.ev
; Claim: sat_format_five_feedback
;
; state = Format(5), last_results = [StringResult("5")]
; => state_next = Count(4)
;    effects = [Println("tick 5")]  (see expected_effects.txt)

(assert (= state_next (Count 4)))
(assert (= effects (seq.unit (Println "tick 5"))))
