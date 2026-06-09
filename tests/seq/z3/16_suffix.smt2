;; E16 suffix: p[i] = s[slen-plen+i] for i<plen. Mirror of prefix.
;; expect: sat unsat
(define-fun slen () Int 5)
(declare-fun s (Int) Int)(declare-fun p (Int) Int)
(define-fun slit () Bool (and (= (s 0) 1)(= (s 1) 2)(= (s 2) 3)(= (s 3) 4)(= (s 4) 5)))
(define-fun smatch ((plen Int)) Bool (forall ((i Int)) (=> (and (<= 0 i)(< i plen)) (= (p i) (s (+ (- slen plen) i))))))
(push)(assert slit)(assert (= (p 0) 4))(assert (= (p 1) 5))(assert (smatch 2))(check-sat)(pop)
(push)(assert slit)(assert (= (p 0) 4))(assert (= (p 1) 9))(assert (smatch 2))(check-sat)(pop)
