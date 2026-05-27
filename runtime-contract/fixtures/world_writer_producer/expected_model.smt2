; expected_model.smt2 — golden witness for world_writer_producer
; Captured from: evident test runtime-contract/fixtures/world_writer_producer/source.ev
; Claim: sat_producer_writes_n
; state=PTick(3) => world_next.n=3, state_next=PTick(2), effects=[]

(assert (= |world_next.n| 3))
(assert (= state_next (PTick 2)))
(assert (= effects (as seq.empty (Seq Effect))))
