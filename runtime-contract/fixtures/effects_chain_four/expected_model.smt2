; expected_model.smt2 — golden witness for effects_chain_four
; Captured from: evident test runtime-contract/fixtures/effects_chain_four/source.ev
; Claim: sat_init_emits_chain_then_exit
;
; Given state=Init, last_results=⟨⟩:
;   state_next = Done
;   effects    = ⟨Println("first"), Println("second"), Println("third"), Exit(0)⟩

(assert (= state_next Done))
(assert (= effects
  (seq.++ (seq.unit (Println "first"))
  (seq.++ (seq.unit (Println "second"))
  (seq.++ (seq.unit (Println "third"))
          (seq.unit (Exit 0)))))))
