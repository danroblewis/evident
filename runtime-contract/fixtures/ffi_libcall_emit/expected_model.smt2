; ffi_libcall_emit — golden next-state witness
; Captured from: evident test runtime-contract/fixtures/ffi_libcall_emit/source.ev
; Claim: sat_emits_libcall
;
; state = FStart
;   => state_next = FDone
;      effects = ⟨LibCall("libc", "abs", "i(i)", ⟨ArgInt(-7)⟩)⟩
(assert (= state_next FDone))
(assert (= effects (seq.unit (LibCall "libc" "abs" "i(i)" (seq.unit (ArgInt (- 7)))))))
