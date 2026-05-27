; effects_chain_four — transition relation for fsm seq_demo, state=Init
; Derived from test_03_seq_chain. how_built: handwritten.
;
; Concatenate with prev.smt2 + inputs.smt2 then append (check-sat)/(get-model).
; No check-sat here.

; ── Sort declarations ──────────────────────────────────────────────────────

(declare-datatypes
  ((SeqState 0) (Effect 0) (Result 0))
  (
    ; SeqState = Init | Done
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
(declare-const state        SeqState)
(declare-const last_results (Seq Result))
(declare-const is_first_tick Bool)

; Outputs (constrained by this relation)
(declare-const state_next   SeqState)
(declare-const effects      (Seq Effect))

; ── Transition constraints ─────────────────────────────────────────────────

; state_next = match state
;   Init => Done
;   Done => Done
(assert (= state_next Done))

; effects = match state
;   Init => ⟨Println("first"), Println("second"), Println("third"), Exit(0)⟩
;   Done => ⟨⟩
(assert (= effects
  (ite (is-Init state)
       (seq.++ (seq.unit (Println "first"))
       (seq.++ (seq.unit (Println "second"))
       (seq.++ (seq.unit (Println "third"))
               (seq.unit (Exit 0)))))
       (as seq.empty (Seq Effect)))))
