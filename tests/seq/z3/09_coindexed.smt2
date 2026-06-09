;; C9 coindexed/zip: parallel walk of xs,ys (N=3) with ys[i]=xs[i]+1.
;; expect: sat unsat
(define-fun N () Int 3)
(declare-fun xs (Int) Int)(declare-fun ys (Int) Int)
(define-fun xlit () Bool (and (= (xs 0) 1)(= (xs 1) 2)(= (xs 2) 3)))
(define-fun zip () Bool (forall ((i Int)) (=> (and (<= 0 i)(< i N)) (= (ys i) (+ (xs i) 1)))))
(push)(assert xlit)(assert (= (ys 0) 2))(assert (= (ys 1) 3))(assert (= (ys 2) 4))(assert zip)(check-sat)(pop)
(push)(assert xlit)(assert (= (ys 0) 5))(assert (= (ys 1) 5))(assert (= (ys 2) 5))(assert zip)(check-sat)(pop)
