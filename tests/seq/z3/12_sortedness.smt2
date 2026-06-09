;; D12 sortedness ∀ i : xs[i] ≤ xs[i+1] over a free bounded seq (N=4).
;; expect: sat unsat
(define-fun N () Int 4)
(declare-fun s (Int) Int)
(define-fun sorted () Bool (forall ((i Int)) (=> (and (<= 0 i)(< i (- N 1))) (<= (s i) (s (+ i 1))))))
(push)(assert sorted)(assert (= (s 0) 1))(assert (= (s 3) 9))(check-sat)(pop)
(push)(assert sorted)(assert (> (s 0) (s 1)))(check-sat)(pop)
