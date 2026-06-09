;; A1 ordering chain: positions of <a,b,c> encode relative order a<b<c.
;; Bounded: 3 named ints; order is a quantifier-free position constraint.
;; expect: sat unsat
(declare-const a Int)(declare-const b Int)(declare-const c Int)
(push)                                  ; positive: chain is satisfiable
(assert (< a b))(assert (< b c))(assert (= a 1))(assert (= c 10))
(check-sat)
(pop)
(push)                                  ; negative: chain + a>c is unsat
(assert (< a b))(assert (< b c))(assert (> a c))
(check-sat)
(pop)
