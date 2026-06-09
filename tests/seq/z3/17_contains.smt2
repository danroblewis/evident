;; E17 contains (contiguous): ∃ off : ∀ j<sublen : s[off+j]=sub[j].
;; expect: sat unsat
(define-fun slen () Int 5)(define-fun sublen () Int 2)
(declare-fun s (Int) Int)(declare-fun sub (Int) Int)
(define-fun slit () Bool (and (= (s 0) 1)(= (s 1) 2)(= (s 2) 3)(= (s 3) 4)(= (s 4) 5)))
(define-fun found () Bool (exists ((off Int))
  (and (<= 0 off) (<= off (- slen sublen))
       (forall ((j Int)) (=> (and (<= 0 j)(< j sublen)) (= (s (+ off j)) (sub j)))))))
(push)(assert slit)(assert (= (sub 0) 3))(assert (= (sub 1) 4))(assert found)(check-sat)(pop)
(push)(assert slit)(assert (= (sub 0) 9))(assert (= (sub 1) 9))(assert found)(check-sat)(pop)
