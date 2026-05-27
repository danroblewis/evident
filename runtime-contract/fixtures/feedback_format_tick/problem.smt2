; feedback_format_tick — transition relation for one tick of fsm `counter`
; FSM source: runtime-contract/fixtures/feedback_format_tick/source.ev
; Derived from: examples/test_02_counter.ev  claim sat_format_five_feedback
;
; Concatenate:  problem.smt2 ++ prev.smt2 ++ inputs.smt2
; Then append:  (check-sat) / (get-model) / uniqueness assertions
; None of these files contain (check-sat).
;
; effects_in_smt: true — the effects Seq is encoded below (the last_results read
; feeds Println("tick " ++ n_str)), so both SMT engines decode it from the model.

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
(declare-const effects    (Seq Effect))

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

; effects = match state
;   Start     => ⟨Println("starting count")⟩
;   Count(n)  => ⟨IntToStr(n)⟩
;   Format(_) => ⟨Println("tick " ++ n_str)⟩   ← reads last_results[0] via n_str
;   Done      => ⟨Println("bye"), Exit(0)⟩
(assert (= effects
  (ite (is-Start  state) (seq.unit (Println "starting count"))
  (ite (is-Count  state) (seq.unit (IntToStr (Count_0 state)))
  (ite (is-Format state) (seq.unit (Println (str.++ "tick " n_str)))
                          (seq.++ (seq.unit (Println "bye")) (seq.unit (Exit 0))))))))
