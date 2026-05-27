; Feedback-loop FSM — the last_results threading worked fixture.
;
; Demonstrates the engine threading effect RESULTS across ticks: tick K emits an
; IntToStr(n) effect, the dispatcher maps it to StringResult("<n>"), and that
; becomes tick K+1's `last_results` input. The FSM reads `last_results[0]` and
; Println's the threaded string. The run prints the formatted int, then exits.
;
; Phase counter `_phase -> phase` (init 0) drives a 3-step program:
;   phase 0: emit IntToStr(42)                         -> last_results = [StringResult("42")]
;   phase 1: read last_results[0]=StringResult("42"),
;            emit Println("42")                         -> last_results = [NoResult]
;   phase 2: emit Println("bye") + Exit(0)              -> halt
;
; The `last_results` metadata names the (Seq Result) const the FSM reads; the
; engine pins the prior tick's dispatched results there (empty seq on tick 0).
; @meta
; {
;   "fsms": [
;     { "name": "feedback",
;       "state": [{"prev":"_phase","next":"phase","sort":"Int","init":0}],
;       "effects": {"var":"effects"},
;       "last_results": {"var":"last_results","elem_sort":"Result"} }
;   ]
; }
; @end
; @transition feedback
(declare-datatypes ((Effect 0))
  (((Println (Println_0 String)) (Exit (Exit_0 Int)) (IntToStr (IntToStr_0 Int)))))
(declare-datatypes ((Result 0))
  (((NoResult) (IntResult (IntResult_0 Int)) (StringResult (StringResult_0 String))
    (ErrorResult (ErrorResult_0 String)))))
(declare-const _phase Int)
(declare-const phase Int)
(declare-const last_results (Seq Result))
(declare-const effects (Seq Effect))
(assert (= phase (+ _phase 1)))
; The string carried in last_results[0] when it is a StringResult; "" otherwise.
(define-fun lr0_str () String
  (ite (and (> (seq.len last_results) 0)
            ((_ is StringResult) (seq.nth last_results 0)))
       (StringResult_0 (seq.nth last_results 0))
       ""))
(assert (= effects
  (ite (= _phase 0)
       (seq.unit (IntToStr 42))
  (ite (= _phase 1)
       (seq.unit (Println lr0_str))
       (seq.++ (seq.unit (Println "bye")) (seq.unit (Exit 0)))))))
