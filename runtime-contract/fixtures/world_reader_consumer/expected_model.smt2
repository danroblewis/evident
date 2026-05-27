; expected_model.smt2 — golden witness for world_reader_consumer
; Captured from: evident test runtime-contract/fixtures/world_reader_consumer/source.ev
; Claim: sat_consumer_emits_int_to_str_when_n_positive
; state=CWait, world.n=7 => state_next=CFormat, effects=[IntToStr(7)]

(assert (= state_next CFormat))
(assert (= effects (seq.unit (IntToStr 7))))
