; expected_model.smt2 — golden witness for tick_exit_42
; Captured from: evident test runtime-contract/fixtures/tick_exit_42/source.ev
; Claim: sat_init_exits_42
; state=Init => state_next=Done, effects=[Println("exiting with code 42"), Exit(42)]

(assert (= state_next Done))
(assert (= effects
  (seq.++ (seq.unit (Println "exiting with code 42"))
          (seq.unit (Exit 42)))))
