;; Auxiliary inductive lemma for z3_numerals_carry's Z3Numerals invariant.
;; The cached 0..4 numeral handles latch in step order (zero@1 … four@5).
;; `four>0 ⇒ zero>0 ∧ one>0 ∧ two>0 ∧ three>0` is sound but not 1-inductive
;; without this step↔field correspondence.
(assert (>= _step 0))
(assert (=> (>= _step 1) (> _z3nums_zero 0)))
(assert (=> (>= _step 2) (> _z3nums_one 0)))
(assert (=> (>= _step 3) (> _z3nums_two 0)))
(assert (=> (>= _step 4) (> _z3nums_three 0)))
(assert (=> (>= _step 5) (> _z3nums_four 0)))
