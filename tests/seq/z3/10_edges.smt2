;; C10 consecutive pairs edges(xs): (xs[i], xs[i+1]) for i<N-1.
;; expect: sat unsat
(define-fun N () Int 3)
(declare-fun s (Int) Int)
(define-fun incr () Bool (forall ((i Int)) (=> (and (<= 0 i)(< i (- N 1))) (< (s i) (s (+ i 1))))))
(push)(assert (= (s 0) 1))(assert (= (s 1) 2))(assert (= (s 2) 3))(assert incr)(check-sat)(pop)
(push)(assert (= (s 0) 3))(assert (= (s 1) 2))(assert (= (s 2) 1))(assert incr)(check-sat)(pop)
