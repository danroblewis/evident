; prev_increment — transition relation for `counter` FSM (test_19_prev_tick)
; Scalar nucleus only; effects involve string ops and are not encoded here.
; Concatenate with prev.smt2 + inputs.smt2, then append (check-sat).

(declare-datatypes
  ((CounterState 0))
  (((Counting) (Done))))

; Infrastructure consts — pinned by prev.smt2 / inputs.smt2
(declare-const is_first_tick Bool)
(declare-const _count        Int)
(declare-const state         CounterState)

; Derived locals
(declare-const count      Int)
(declare-const state_next CounterState)

; count = is_first_tick ? 0 : _count + 1
(assert (= count (ite is_first_tick 0 (+ _count 1))))

; state_next = count >= 3 ? Done : Counting
(assert (= state_next (ite (>= count 3) Done Counting)))
