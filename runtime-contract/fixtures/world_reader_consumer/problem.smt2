; world_reader_consumer — transition relation for one tick of fsm `consumer`
; FSM source: runtime-contract/fixtures/world_reader_consumer/source.ev
; Derived from: examples/test_09_two_fsms.ev  claim sat_consumer_emits_int_to_str_when_n_positive
;
; Concatenate:  problem.smt2 ++ prev.smt2 ++ inputs.smt2
; Then append:  (check-sat) / (get-model) / uniqueness assertions
; None of these files contain (check-sat).

; ── Datatype declarations ─────────────────────────────────────────────────────

(declare-datatypes
  ((PState 0) (CState 0) (Effect 0) (Result 0))
  (((PStart) (PTick (PTick_0 Int)) (PEnd))
   ((CWait) (CFormat) (CEnd))
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

; ── World fields (flattened as scalar consts) ──────────────────────────────────
; Pipe-quoted to preserve dotted names

(declare-const |world.n| Int)

; ── Infrastructure constants ───────────────────────────────────────────────────

(declare-const state        CState)
(declare-const state_next   CState)
(declare-const effects      (Seq Effect))
(declare-const last_results (Seq Result))
(declare-const n_str        String)

; ── Transition constraints ─────────────────────────────────────────────────────

; n_str = match last_results[0]
;   StringResult(s) => s
;   _               => "?"
; When last_results is empty, seq.nth returns arbitrary value; n_str unconstrained.
; (n_str is only used in CFormat branch; pinned state=CWait so n_str does not affect golden)
(assert (= n_str
  (ite (is-StringResult (seq.nth last_results 0))
       (StringResult_0 (seq.nth last_results 0))
       "?")))

; state_next = match state
;   CWait   => (world.n > 0 ? CFormat : CWait)
;   CFormat => CWait
;   CEnd    => CEnd
(assert (= state_next
  (ite (is-CWait   state) (ite (> |world.n| 0) CFormat CWait)
  (ite (is-CFormat state) CWait
                          CEnd))))

; effects = match state
;   CWait   => (world.n > 0 ? [IntToStr(world.n)] : [])
;   CFormat => [Println("consumer saw n = " ++ n_str)]
;   CEnd    => []
(assert (= effects
  (ite (is-CWait   state) (ite (> |world.n| 0)
                               (seq.unit (IntToStr |world.n|))
                               (as seq.empty (Seq Effect)))
  (ite (is-CFormat state) (seq.unit (Println (str.++ "consumer saw n = " n_str)))
                          (as seq.empty (Seq Effect))))))
