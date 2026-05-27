; feedback_int_payload — transition relation for one tick of fsm `accumulator`
; FSM source: runtime-contract/fixtures/feedback_int_payload/source.ev
; Derived from: source.ev  claim sat_ready_increments
;
; Concatenate:  problem.smt2 ++ prev.smt2 ++ inputs.smt2
; Then append:  (check-sat) / (get-model) / uniqueness assertions
; None of these files contain (check-sat).
;
; effects_in_smt: true — the IntToStr(n+1) effect uses the Int payload read from
; last_results[0], so both SMT engines decode it from the model.

; ── Datatype declarations ─────────────────────────────────────────────────────

(declare-datatypes
  ((AState 0) (Effect 0) (Result 0))
  (((AStart) (AReady) (ADone))
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

(declare-const state      AState)
(declare-const state_next AState)
(declare-const last_results (Seq Result))
(declare-const effects    (Seq Effect))

; n: the Int payload read from last_results[0]
(declare-const n Int)

; ── Transition constraints ────────────────────────────────────────────────────

; n = match last_results[0] { IntResult(v) => v ; _ => 0 }
(assert (= n
  (ite (is-IntResult (seq.nth last_results 0))
       (IntResult_0 (seq.nth last_results 0))
       0)))

; state_next = match state { AStart => AReady ; AReady => ADone ; ADone => ADone }
(assert (= state_next
  (ite (is-AStart state) AReady
  (ite (is-AReady state) ADone
                         ADone))))

; effects = match state
;   AStart => ⟨ParseInt("41")⟩
;   AReady => ⟨IntToStr(n + 1)⟩   ← uses the Int payload threaded via last_results
;   ADone  => ⟨⟩
(assert (= effects
  (ite (is-AStart state) (seq.unit (ParseInt "41"))
  (ite (is-AReady state) (seq.unit (IntToStr (+ n 1)))
                         (as seq.empty (Seq Effect))))))
