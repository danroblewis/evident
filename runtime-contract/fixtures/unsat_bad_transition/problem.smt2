; unsat_bad_transition — transition relation for one tick of fsm `counter`
; FSM source: runtime-contract/fixtures/unsat_bad_transition/source.ev
; Derived from: examples/test_02_counter.ev  claim unsat_count_increments
;
; Cluster F (negative / UNSAT): state=Format(3) forces state_next=Count(2).
; inputs.smt2 additionally pins state_next=Count(4), creating a contradiction.
;
; Concatenate:  problem.smt2 ++ prev.smt2 ++ inputs.smt2
; Then append:  (check-sat)
; None of these files contain (check-sat).

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

(declare-const state        CountState)
(declare-const state_next   CountState)
(declare-const last_results (Seq Result))

; n_str — derived from last_results[0]; used only in Format arm
(declare-const n_str String)

; ── Transition constraints ────────────────────────────────────────────────────

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

; n_str = match last_results[0]
;   StringResult(s) => s
;   _               => "?"
(assert (= n_str
  (ite (and (>= (seq.len last_results) 1)
            (is-StringResult (seq.nth last_results 0)))
       (StringResult_0 (seq.nth last_results 0))
       "?")))
