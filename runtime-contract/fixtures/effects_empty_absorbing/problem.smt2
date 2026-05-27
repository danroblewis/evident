; effects_empty_absorbing — transition relation for fsm hello, state=Done
; Derived from test_01_hello. how_built: handwritten.
;
; Concatenate with prev.smt2 + inputs.smt2 then append (check-sat)/(get-model).
; No check-sat here.

; ── Sort declarations ──────────────────────────────────────────────────────

(declare-datatypes
  ((HelloState 0) (Effect 0) (Result 0))
  (
    ; HelloState = Init | Done
    ((Init) (Done))

    ; Effect
    ((NoEffect)
     (Print   (Print_0   String))
     (Println (Println_0 String))
     (ReadLine)
     (Time)
     (Exit    (Exit_0    Int))
     (ParseInt   (ParseInt_0   String))
     (ParseReal  (ParseReal_0  String))
     (IntToStr   (IntToStr_0   Int))
     (RealToStr  (RealToStr_0  Real))
     (ShellRun   (ShellRun_0   String))
     (SpawnFsm   (SpawnFsm_0   String) (SpawnFsm_1 Int))
     (MonotonicTime))

    ; Result
    ((NoResult)
     (IntResult    (IntResult_0    Int))
     (StringResult (StringResult_0 String))
     (BoolResult   (BoolResult_0   Bool))
     (RealResult   (RealResult_0   Real))
     (HandleResult (HandleResult_0 Int))
     (ErrorResult  (ErrorResult_0  String)))
  )
)

; ── FSM variables ──────────────────────────────────────────────────────────

; Inputs (pinned by prev.smt2 / inputs.smt2)
(declare-const state        HelloState)
(declare-const last_results (Seq Result))
(declare-const is_first_tick Bool)

; Outputs (constrained by this relation)
(declare-const state_next   HelloState)
(declare-const effects      (Seq Effect))

; ── Transition constraints ─────────────────────────────────────────────────

; state_next = match state
;   Init => Done
;   Done => Done
(assert (= state_next Done))

; effects = match state
;   Init => ⟨Println("hello from evident"), Exit(0)⟩
;   Done => ⟨⟩
(assert (= effects
  (ite (is-Init state)
       (seq.++ (seq.unit (Println "hello from evident"))
               (seq.unit (Exit 0)))
       (as seq.empty (Seq Effect)))))
