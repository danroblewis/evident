; expected_model.smt2 — golden witness for tick_hello_init
; Captured from: evident test runtime-contract/fixtures/tick_hello_init/source.ev
; Claim: sat_init_advances_to_done
; state=Init => state_next=Done, effects=[Println("hello from evident"), Exit(0)]

(assert (= state_next Done))
(assert (= effects
  (seq.++ (seq.unit (Println "hello from evident"))
          (seq.unit (Exit 0)))))
