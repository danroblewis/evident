; feedback_parse_read — transition relation for one tick of fsm `parse_demo`
; FSM source: runtime-contract/fixtures/feedback_parse_read/source.ev
; Derived from: examples/test_04_parse_int.ev  claim sat_read_feedback
;
; Concatenate:  problem.smt2 ++ prev.smt2 ++ inputs.smt2
; Then append:  (check-sat) / (get-model) / uniqueness assertions
; None of these files contain (check-sat).
;
; effects_in_smt: true — the effects Seq is encoded below (the Read arm prints
; good/bad, both read from last_results), so both SMT engines decode it.

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
(declare-const effects    (Seq Effect))

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

; effects = match state
;   Issue => ⟨ParseInt("42"), ParseInt("not-a-number")⟩
;   Read  => ⟨Println(good), Println(bad), Exit(0)⟩   ← good/bad read from last_results
;   Done  => ⟨⟩
(assert (= effects
  (ite (is-Issue state)
       (seq.++ (seq.unit (ParseInt "42")) (seq.unit (ParseInt "not-a-number")))
  (ite (is-Read  state)
       (seq.++ (seq.unit (Println good))
               (seq.++ (seq.unit (Println bad)) (seq.unit (Exit 0))))
       (as seq.empty (Seq Effect))))))
