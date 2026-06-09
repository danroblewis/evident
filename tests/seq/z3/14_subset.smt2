;; D14 subset ∀ x∈xs : x∈someSet. Set modelled as a membership predicate
;; (a bounded disjunction). In Evident this compiles via Set membership.
;; expect: sat unsat
(define-fun N () Int 2)
(declare-fun s (Int) Int)
(define-fun inset ((v Int)) Bool (or (= v 1)(= v 2)(= v 3)(= v 4)(= v 5)))
(define-fun subset () Bool (forall ((i Int)) (=> (and (<= 0 i)(< i N)) (inset (s i)))))
(push)(assert (= (s 0) 2))(assert (= (s 1) 4))(assert subset)(check-sat)(pop)
(push)(assert (= (s 0) 2))(assert (= (s 1) 9))(assert subset)(check-sat)(pop)
