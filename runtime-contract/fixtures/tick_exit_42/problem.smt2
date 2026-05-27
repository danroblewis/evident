; tick_exit_42 — transition relation for one tick of fsm `exit_demo`
; FSM source: runtime-contract/fixtures/tick_exit_42/source.ev
; Derived from: examples/test_08_exit_code.ev  claim sat_init_exits_42
;
; Concatenate:  problem.smt2 ++ prev.smt2 ++ inputs.smt2
; Then append:  (check-sat) / (get-model) / uniqueness assertions
; None of these files contain (check-sat).

; ── Datatype declarations ─────────────────────────────────────────────────────

(declare-datatypes
  ((XState 0) (Effect 0) (Result 0))
  (((Init) (Done))
   ((NoEffect)
    (Print    (Print_0    String))
    (Println  (Println_0  String))
    (ReadLine)
    (Time)
    (Exit     (Exit_0     Int))
    (ParseInt (ParseInt_0 String))
    (IntToStr (IntToStr_0 Int)))
   ((NoResult)
    (IntResult    (IntResult_0    Int))
    (StringResult (StringResult_0 String))
    (BoolResult   (BoolResult_0   Bool))
    (RealResult   (RealResult_0   Real))
    (HandleResult (HandleResult_0 Int))
    (ErrorResult  (ErrorResult_0  String)))))

; ── Infrastructure constants ──────────────────────────────────────────────────

(declare-const state        XState)
(declare-const state_next   XState)
(declare-const effects      (Seq Effect))
(declare-const last_results (Seq Result))

; ── Transition constraints ────────────────────────────────────────────────────

; state_next = match state { Init => Done; Done => Done }
(assert (= state_next
  (ite (is-Init state) Done
                       Done)))

; effects = match state
;   Init => [Println("exiting with code 42"), Exit(42)]
;   Done => []
(assert (= effects
  (ite (is-Init state)
       (seq.++ (seq.unit (Println "exiting with code 42"))
               (seq.unit (Exit 42)))
       (as seq.empty (Seq Effect)))))
