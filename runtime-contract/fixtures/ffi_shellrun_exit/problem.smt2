; ffi_shellrun_exit — transition relation for one tick of fsm `shell_demo`
; FSM source: runtime-contract/fixtures/ffi_shellrun_exit/source.ev
; Derived from: source.ev  claim sat_runs_then_exits
;
; Concatenate:  problem.smt2 ++ prev.smt2 ++ inputs.smt2
; Then append:  (check-sat) / (get-model) / uniqueness assertions
;
; effects_in_smt: true — the ShellRun + Exit chain is encoded in the effects Seq;
; both SMT engines decode it. (The ShellRun's C side effect is dispatch, not
; transition; the contract unit is the emitted, ordered effect list.)

(declare-datatypes
  ((SState 0) (Effect 0))
  (((SRun) (SDone))
   ((NoEffect)
    (Print    (Print_0    String))
    (Println  (Println_0  String))
    (Exit     (Exit_0     Int))
    (IntToStr (IntToStr_0 Int))
    (ShellRun (ShellRun_0 String)))))

(declare-const state      SState)
(declare-const state_next SState)
(declare-const effects    (Seq Effect))

; state_next = match state { SRun => SDone ; SDone => SDone }
(assert (= state_next (ite (is-SRun state) SDone SDone)))

; effects = match state
;   SRun  => ⟨ShellRun("echo hi"), Exit(0)⟩
;   SDone => ⟨⟩
(assert (= effects
  (ite (is-SRun state)
       (seq.++ (seq.unit (ShellRun "echo hi")) (seq.unit (Exit 0)))
       (as seq.empty (Seq Effect)))))
