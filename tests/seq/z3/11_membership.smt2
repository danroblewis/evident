;; C11 membership x∈xs, encoded the decidable bounded way:
;; ∃ i<N : xs[i]=target  (Evident drops the direct ∈ on a Seq).
;; expect: sat unsat
(define-fun N () Int 3)
(declare-fun s (Int) Int)
(define-fun lit () Bool (and (= (s 0) 1)(= (s 1) 2)(= (s 2) 3)))
(push)(assert lit)(assert (exists ((i Int)) (and (<= 0 i)(< i N)(= (s i) 2))))(check-sat)(pop)
(push)(assert lit)(assert (exists ((i Int)) (and (<= 0 i)(< i N)(= (s i) 99))))(check-sat)(pop)
