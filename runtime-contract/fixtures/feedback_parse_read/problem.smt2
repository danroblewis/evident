; feedback_parse_read — transition relation for one tick of fsm `parse_demo`
; FSM source: runtime-contract/fixtures/feedback_parse_read/source.ev
; Derived from: examples/test_04_parse_int.ev  claim sat_read_feedback
;
; Concatenate:  problem.smt2 ++ prev.smt2 ++ inputs.smt2
; Then append:  (check-sat) / (get-model) / uniqueness assertions
; None of these files contain (check-sat).
;
; effects_in_smt: false — effects not encoded here; see expected_effects.txt

; ── Datatype declarations ─────────────────────────────────────────────────────

(declare-datatypes
  ((PState 0) (Effect 0) (Result 0))
  (((Issue) (Read) (Done))
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
; prev.smt2 pins state; inputs.smt2 pins last_results

(declare-const state      PState)
(declare-const state_next PState)
(declare-const last_results (Seq Result))

; good: derived from last_results[0]
(declare-const good String)
; bad: derived from last_results[1]
(declare-const bad String)

; ── Transition constraints ────────────────────────────────────────────────────

; good = match last_results[0]
;   IntResult(_)   => "good: parsed an Int"
;   ErrorResult(_) => "good: ERROR was expected to be success"
;   _              => "good: unknown result"
(assert (= good
  (ite (is-IntResult   (seq.nth last_results 0)) "good: parsed an Int"
  (ite (is-ErrorResult (seq.nth last_results 0)) "good: ERROR was expected to be success"
       "good: unknown result"))))

; bad = match last_results[1]
;   IntResult(_)   => "bad: parsed but expected error"
;   ErrorResult(_) => "bad: ERROR was correct"
;   _              => "bad: unknown"
(assert (= bad
  (ite (is-IntResult   (seq.nth last_results 1)) "bad: parsed but expected error"
  (ite (is-ErrorResult (seq.nth last_results 1)) "bad: ERROR was correct"
       "bad: unknown"))))

; state_next = match state
;   Issue => Read
;   Read  => Done
;   Done  => Done
(assert (= state_next
  (ite (is-Issue state) Read
  (ite (is-Read  state) Done
                        Done))))
