;; B5 concat a(2)++b(2)++<x> preserves order. Model the result c of len 5
;; whose slots equal the source elements in order.
;; expect: sat unsat
(declare-fun c (Int) Int)
(declare-const a0 Int)(declare-const a1 Int)(declare-const b0 Int)(declare-const b1 Int)(declare-const x Int)
(define-fun glued () Bool (and (= (c 0) a0)(= (c 1) a1)(= (c 2) b0)(= (c 3) b1)(= (c 4) x)))
(push)                                  ; positive: order preserved
(assert glued)(assert (= a0 1))(assert (= a1 2))(assert (= b0 3))(assert (= b1 4))(assert (= x 5))
(assert (= (c 2) 3))(assert (= (c 4) 5))
(check-sat)(pop)
(push)                                  ; negative: claim c[2] is the wrong source
(assert glued)(assert (= b0 3))(assert (= (c 2) 99))
(check-sat)(pop)
