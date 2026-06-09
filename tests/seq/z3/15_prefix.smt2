;; E15 prefix: plen≤slen ∧ ∀ i<plen : p[i]=s[i].  Cheap when bounded.
;; expect: sat unsat
(declare-fun s (Int) Int)(declare-fun p (Int) Int)
(define-fun slit () Bool (and (= (s 0) 1)(= (s 1) 2)(= (s 2) 3)(= (s 3) 4)(= (s 4) 5)))
(define-fun pmatch ((plen Int)) Bool (forall ((i Int)) (=> (and (<= 0 i)(< i plen)) (= (p i) (s i)))))
(push)(assert slit)(assert (<= 3 5))(assert (= (p 0) 1))(assert (= (p 1) 2))(assert (= (p 2) 3))(assert (pmatch 3))(check-sat)(pop)
(push)(assert slit)(assert (= (p 0) 1))(assert (= (p 1) 9))(assert (= (p 2) 3))(assert (pmatch 3))(check-sat)(pop)
