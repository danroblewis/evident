; feedback_int_payload — golden next-state witness
; Captured from: evident test runtime-contract/fixtures/feedback_int_payload/source.ev
; Claim: sat_ready_increments
;
; state = AReady, last_results = [IntResult(41)]
;   => n = 41, state_next = ADone, effects = [IntToStr(42)]
(assert (= state_next ADone))
(assert (= effects (seq.unit (IntToStr 42))))
