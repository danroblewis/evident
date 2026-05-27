; feedback_parse_read — golden next-state witness
; Captured from: evident test runtime-contract/fixtures/feedback_parse_read/source.ev
; Claim: sat_read_feedback
;
; state = Read, last_results = [IntResult(42), ErrorResult("invalid digit")]
; => state_next = Done
;    effects = [Println("good: parsed an Int"), Println("bad: ERROR was correct"), Exit(0)]
;    (see expected_effects.txt)

(assert (= state_next Done))
