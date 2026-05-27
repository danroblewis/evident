; prev_increment — transition relation for `counter` FSM (test_19_prev_tick)
; Concatenate with prev.smt2 + inputs.smt2, then append (check-sat).
; effects_in_smt: true — the full effects Seq (Println of prev_str + IntToStr(count))
; is encoded; last_results is pinned (empty) so prev_str="?" is deterministic.

(declare-datatypes
  ((CounterState 0) (Effect 0) (Result 0))
  (((Counting) (Done))
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

; Infrastructure consts — pinned by prev.smt2 / inputs.smt2
(declare-const is_first_tick Bool)
(declare-const _count        Int)
(declare-const state         CounterState)
(declare-const last_results  (Seq Result))

; Derived locals
(declare-const count      Int)
(declare-const state_next CounterState)
(declare-const has_result Bool)
(declare-const first_str  String)
(declare-const prev_str   String)
(declare-const effects    (Seq Effect))

; count = is_first_tick ? 0 : _count + 1
(assert (= count (ite is_first_tick 0 (+ _count 1))))

; state_next = count >= 3 ? Done : Counting
(assert (= state_next (ite (>= count 3) Done Counting)))

; has_result = #last_results > 0 ; first_str = match last_results[1] ; prev_str = has_result ? first_str : "?"
(assert (= has_result (> (seq.len last_results) 0)))
(assert (= first_str
  (ite (and (>= (seq.len last_results) 2) (is-StringResult (seq.nth last_results 1)))
       (StringResult_0 (seq.nth last_results 1)) "?")))
(assert (= prev_str (ite has_result first_str "?")))

; effects = match state
;   Counting => ⟨Println("count = " ++ prev_str), IntToStr(count)⟩
;   Done     => ⟨Println("done"), Exit(0)⟩
(assert (= effects
  (ite (is-Counting state)
       (seq.++ (seq.unit (Println (str.++ "count = " prev_str))) (seq.unit (IntToStr count)))
       (seq.++ (seq.unit (Println "done")) (seq.unit (Exit 0))))))
