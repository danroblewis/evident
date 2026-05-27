; tick_hello_init — transition relation for one tick of fsm `hello`
; FSM source: runtime-contract/fixtures/tick_hello_init/source.ev
; Derived from: examples/test_01_hello.ev  claim sat_init_advances_to_done
;
; Concatenate:  problem.smt2 ++ prev.smt2 ++ inputs.smt2
; Then append:  (check-sat) / (get-model) / uniqueness assertions
; None of these files contain (check-sat).

; ── Datatype declarations ─────────────────────────────────────────────────────

(declare-datatypes
  ((HelloState 0) (Effect 0) (Result 0))
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
; prev.smt2 will pin state; inputs.smt2 will pin last_results length

(declare-const state      HelloState)
(declare-const state_next HelloState)
(declare-const effects    (Seq Effect))

; last_results is empty for this fixture; declare length for completeness
(declare-const last_results (Seq Result))

; ── Transition constraints ────────────────────────────────────────────────────

; state_next = match state { Init => Done; Done => Done }
(assert (= state_next
  (ite (is-Init state) Done
                       Done)))

; effects = match state { Init => [Println("hello from evident"), Exit(0)]; Done => [] }
(assert (= effects
  (ite (is-Init state)
       (seq.++ (seq.unit (Println "hello from evident"))
               (seq.unit (Exit 0)))
       (as seq.empty (Seq Effect)))))
