; ffi_shellrun_exit — golden next-state witness
; Captured from: evident test runtime-contract/fixtures/ffi_shellrun_exit/source.ev
; Claim: sat_runs_then_exits
;
; state = SRun => state_next = SDone, effects = ⟨ShellRun("echo hi"), Exit(0)⟩
(assert (= state_next SDone))
(assert (= effects (seq.++ (seq.unit (ShellRun "echo hi")) (seq.unit (Exit 0)))))
