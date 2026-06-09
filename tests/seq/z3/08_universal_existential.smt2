;; C8 universal/existential over a bounded seq (N=3).
;; expect: sat unsat sat unsat
(define-fun N () Int 3)
(declare-fun s (Int) Int)
(define-fun lit () Bool (and (= (s 0) 1)(= (s 1) 2)(= (s 2) 3)))
(define-fun allpos () Bool (forall ((i Int)) (=> (and (<= 0 i) (< i N)) (> (s i) 0))))
(push)(assert lit)(assert allpos)(check-sat)(pop)                         ; ∀ x>0 holds
(push)(assert (= (s 0) 1))(assert (= (s 1) (- 5)))(assert allpos)(check-sat)(pop) ; ∀ x>0 with a negative elt
(push)(assert lit)(assert (exists ((i Int)) (and (<= 0 i)(< i N)(> (s i) 2))))(check-sat)(pop) ; ∃ x>2
(push)(assert lit)(assert (exists ((i Int)) (and (<= 0 i)(< i N)(> (s i) 99))))(check-sat)(pop) ; ∃ x>99
