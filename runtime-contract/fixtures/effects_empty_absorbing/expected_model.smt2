; expected_model.smt2 — golden witness for effects_empty_absorbing
; Captured from: evident test runtime-contract/fixtures/effects_empty_absorbing/source.ev
; Claim: sat_done_absorbs
;
; Given state=Done, last_results=⟨⟩:
;   state_next = Done
;   effects    = ⟨⟩  (empty — Done is the absorbing steady state)

(assert (= state_next Done))
(assert (= effects (as seq.empty (Seq Effect))))
