; prev_record_fields — transition relation for `walker` FSM (test_22_prev_record)
; No enum state. Scalar nucleus: record time-shift via per-field pins.
; effects_in_smt: true — the full effects Seq is encoded; last_results is pinned
; to ⟨StringResult("done")⟩ so the body's s=match last_results[0] is deterministic.
; Dotted field names are pipe-quoted per SMT-LIB spec.
; Concatenate with prev.smt2 + inputs.smt2, then append (check-sat).

(declare-datatypes
  ((Effect 0) (Result 0))
  (((NoEffect)
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
(declare-const |_pos.x|      Int)
(declare-const |_pos.y|      Int)
(declare-const last_results  (Seq Result))

; Intermediate: step = _pos + IVec2(1, 2)
(declare-const |step.x| Int)
(declare-const |step.y| Int)
(assert (= |step.x| (+ |_pos.x| 1)))
(assert (= |step.y| (+ |_pos.y| 2)))

; Output: pos = IVec2(is_first_tick ? 0 : step.x, is_first_tick ? 0 : step.y)
(declare-const |pos.x| Int)
(declare-const |pos.y| Int)
(assert (= |pos.x| (ite is_first_tick 0 |step.x|)))
(assert (= |pos.y| (ite is_first_tick 0 |step.y|)))

; halt = pos.x >= 3
(declare-const halt Bool)
(assert (= halt (>= |pos.x| 3)))

; sum = IntToStr(pos.x + pos.y) ; have_prior = #last_results > 0 ;
; s = match last_results[0] { StringResult(v) => v ; _ => "?" }
(declare-const sum        Effect)
(declare-const have_prior Bool)
(declare-const s          String)
(declare-const effects    (Seq Effect))
(assert (= sum (IntToStr (+ |pos.x| |pos.y|))))
(assert (= have_prior (> (seq.len last_results) 0)))
(assert (= s
  (ite (and (>= (seq.len last_results) 1) (is-StringResult (seq.nth last_results 0)))
       (StringResult_0 (seq.nth last_results 0)) "?")))

; effects = halt ? ⟨Println("walker done at " ++ s), Exit(0)⟩
;                : have_prior ? ⟨sum, Println("pos.x+pos.y = " ++ s)⟩ : ⟨sum⟩
(assert (= effects
  (ite halt
    (seq.++ (seq.unit (Println (str.++ "walker done at " s))) (seq.unit (Exit 0)))
    (ite have_prior
      (seq.++ (seq.unit sum) (seq.unit (Println (str.++ "pos.x+pos.y = " s))))
      (seq.unit sum)))))
