;; E18 fixed-window extract: w[j] = s[start+j] for j<wlen.
;; expect: sat unsat
(define-fun wlen () Int 2)(define-fun start () Int 1)
(declare-fun s (Int) Int)(declare-fun w (Int) Int)
(define-fun slit () Bool (and (= (s 0) 10)(= (s 1) 20)(= (s 2) 30)(= (s 3) 40)(= (s 4) 50)))
(define-fun extract () Bool (forall ((j Int)) (=> (and (<= 0 j)(< j wlen)) (= (w j) (s (+ start j))))))
(push)(assert slit)(assert extract)(assert (= (w 0) 20))(assert (= (w 1) 30))(check-sat)(pop)
(push)(assert slit)(assert extract)(assert (= (w 0) 20))(assert (= (w 1) 99))(check-sat)(pop)
