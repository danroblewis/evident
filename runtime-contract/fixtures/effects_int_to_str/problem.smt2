; effects_int_to_str — transition relation for fsm counter, state=Count(3)
; Derived from test_02_counter. how_built: handwritten.
;
; Concatenate with prev.smt2 + inputs.smt2 then append (check-sat)/(get-model).
; No check-sat here.

; ── Sort declarations ──────────────────────────────────────────────────────

(declare-datatypes
  ((CountState 0) (Effect 0) (Result 0))
  (
    ; CountState = Start | Count(Int) | Format(Int) | Done
    ((Start) (Count (Count_0 Int)) (Format (Format_0 Int)) (Done))

    ; Effect (subset used in this fixture; full enum for SMT completeness)
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
(declare-const state        CountState)
(declare-const last_results (Seq Result))
(declare-const is_first_tick Bool)

; Outputs (constrained by this relation)
(declare-const state_next   CountState)
(declare-const effects      (Seq Effect))

; Intermediate
(declare-const n_str String)

; ── Transition constraints ─────────────────────────────────────────────────

; state_next = match state
;   Start     => Count(5)
;   Count(n)  => Format(n)
;   Format(n) => (n <= 1 ? Done : Count(n - 1))
;   Done      => Done
(assert (= state_next
  (ite (is-Start  state) (Count 5)
  (ite (is-Count  state) (Format (Count_0 state))
  (ite (is-Format state) (ite (<= (Format_0 state) 1)
                               Done
                               (Count (- (Format_0 state) 1)))
                          Done)))))

; n_str = match last_results[0]
;   StringResult(s) => s
;   _               => "?"
(assert (= n_str
  (ite (and (>= (seq.len last_results) 1)
            (is-StringResult (seq.nth last_results 0)))
       (StringResult_0 (seq.nth last_results 0))
       "?")))

; effects = match state
;   Start     => ⟨Println("starting count")⟩
;   Count(n)  => ⟨IntToStr(n)⟩
;   Format(_) => ⟨Println("tick " ++ n_str)⟩
;   Done      => ⟨Println("bye"), Exit(0)⟩
(assert (= effects
  (ite (is-Start  state) (seq.unit (Println "starting count"))
  (ite (is-Count  state) (seq.unit (IntToStr (Count_0 state)))
  (ite (is-Format state) (seq.unit (Println (str.++ "tick " n_str)))
                          (seq.++ (seq.unit (Println "bye")) (seq.unit (Exit 0))))))))
