; feedback_format_tick — transition relation for one tick of fsm `counter`
; FSM source: runtime-contract/fixtures/feedback_format_tick/source.ev
; Derived from: examples/test_02_counter.ev  claim sat_format_five_feedback
;
; Concatenate:  problem.smt2 ++ prev.smt2 ++ inputs.smt2
; Then append:  (check-sat) / (get-model) / uniqueness assertions
; None of these files contain (check-sat).
;
; effects_in_smt: false — effects not encoded here; see expected_effects.txt

; ── Datatype declarations ─────────────────────────────────────────────────────

(declare-datatypes
  ((CountState 0) (Effect 0) (Result 0))
  (((Start) (Count (Count_0 Int)) (Format (Format_0 Int)) (Done))
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

(declare-const state      CountState)
(declare-const state_next CountState)
(declare-const last_results (Seq Result))

; n_str: derived local — the string read from last_results[0]
(declare-const n_str String)

; ── Transition constraints ────────────────────────────────────────────────────

; n_str = match last_results[0] { StringResult(s) => s; _ => "?" }
(assert (= n_str
  (ite (is-StringResult (seq.nth last_results 0))
       (StringResult_0 (seq.nth last_results 0))
       "?")))

; state_next = match state
;   Start     => Count(5)
;   Count(n)  => Format(n)
;   Format(n) => (n <= 1 ? Done : Count(n-1))
;   Done      => Done
(assert (= state_next
  (ite (is-Start  state) (Count 5)
  (ite (is-Count  state) (Format (Count_0 state))
  (ite (is-Format state)
       (ite (<= (Format_0 state) 1) Done (Count (- (Format_0 state) 1)))
       Done)))))
