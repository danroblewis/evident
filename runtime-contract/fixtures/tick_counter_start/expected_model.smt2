; expected_model.smt2 — golden witness for tick_counter_start
; Captured from: evident test runtime-contract/fixtures/tick_counter_start/source.ev
; Claim: sat_start_seeds_count_five
; state=Start => state_next=Count(5), effects=[Println("starting count")]

(assert (= state_next (Count 5)))
(assert (= effects (seq.unit (Println "starting count"))))
