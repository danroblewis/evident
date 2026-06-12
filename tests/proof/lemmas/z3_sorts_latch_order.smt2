;; Auxiliary inductive lemma for z3_sorts_carry's Z3Sorts invariant.
;; The four base-sort handles latch in step order (isort@1, bsort@2, ssort@3,
;; rsort@4). `rsort>0 ⇒ isort>0 ∧ bsort>0 ∧ ssort>0` is sound but not 1-inductive
;; without this step↔field correspondence.
(assert (>= _step 0))
(assert (=> (>= _step 1) (> _z3sorts_isort 0)))
(assert (=> (>= _step 2) (> _z3sorts_bsort 0)))
(assert (=> (>= _step 3) (> _z3sorts_ssort 0)))
(assert (=> (>= _step 4) (> _z3sorts_rsort 0)))
