; expected_model.smt2 — golden witness for effects_int_to_str
; Captured from: evident test runtime-contract/fixtures/effects_int_to_str/source.ev
; Claim: sat_count_emits_int_to_str
;
; Given state=Count(3), last_results=⟨⟩:
;   state_next = Format(3)   (match Count(n) => Format(n))
;   effects    = ⟨IntToStr(3)⟩

(assert (= state_next (Format 3)))
(assert (= effects (seq.unit (IntToStr 3))))
