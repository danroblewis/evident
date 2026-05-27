; unsat_hello_done_to_init — transition relation for one tick of fsm `hello`
; FSM source: runtime-contract/fixtures/unsat_hello_done_to_init/source.ev
; Derived from: examples/test_01_hello.ev  claim unsat_done_returns_to_init
;
; Cluster F (negative / UNSAT): state=Done forces state_next=Done (absorbing state).
; inputs.smt2 additionally pins state_next=Init, creating a contradiction.
;
; Concatenate:  problem.smt2 ++ prev.smt2 ++ inputs.smt2
; Then append:  (check-sat)
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

(declare-const state        HelloState)
(declare-const state_next   HelloState)
(declare-const last_results (Seq Result))

; ── Transition constraints ────────────────────────────────────────────────────

; state_next = match state { Init => Done; Done => Done }
; Done is absorbing: both arms yield Done.
(assert (= state_next
  (ite (is-Init state) Done
                       Done)))
