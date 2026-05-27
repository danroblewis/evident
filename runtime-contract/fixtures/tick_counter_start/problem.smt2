; tick_counter_start — transition relation for one tick of fsm `counter`
; FSM source: runtime-contract/fixtures/tick_counter_start/source.ev
; Derived from: examples/test_02_counter.ev  claim sat_start_seeds_count_five
;
; Concatenate:  problem.smt2 ++ prev.smt2 ++ inputs.smt2
; Then append:  (check-sat) / (get-model) / uniqueness assertions
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
(declare-const effects      (Seq Effect))
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
; When last_results is empty, seq.nth returns a default element.
; We use (seq.len last_results) to guard: if length > 0 use the element, else "?".
(assert (= n_str
  (ite (> (seq.len last_results) 0)
       (ite (is-StringResult (seq.nth last_results 0))
            (StringResult_0 (seq.nth last_results 0))
            "?")
       "?")))

; effects = match state
;   Start     => [Println("starting count")]
;   Count(n)  => [IntToStr(n)]
;   Format(_) => [Println("tick " ++ n_str)]
;   Done      => [Println("bye"), Exit(0)]
(assert (= effects
  (ite (is-Start  state)
       (seq.unit (Println "starting count"))
  (ite (is-Count  state)
       (seq.unit (IntToStr (Count_0 state)))
  (ite (is-Format state)
       (seq.unit (Println (str.++ "tick " n_str)))
       (seq.++ (seq.unit (Println "bye"))
               (seq.unit (Exit 0))))))))
