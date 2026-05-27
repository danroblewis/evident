; prev_record_fields — transition relation for `walker` FSM (test_22_prev_record)
; No enum state. Scalar nucleus: record time-shift via per-field pins.
; effects involve string ops and are not encoded here (effects_in_smt: false).
; Dotted field names are pipe-quoted per SMT-LIB spec.
; Concatenate with prev.smt2 + inputs.smt2, then append (check-sat).

; Infrastructure consts — pinned by prev.smt2 / inputs.smt2
(declare-const is_first_tick Bool)
(declare-const |_pos.x|      Int)
(declare-const |_pos.y|      Int)

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
