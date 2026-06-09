;; B4 literal / empty. Empty seq has len 0; a literal pins each slot.
;; expect: sat sat unsat
(declare-fun s (Int) Int)(declare-const n Int)
(push)(assert (= n 0))(check-sat)(pop)                 ; empty: len 0 is fine
(push)                                                  ; literal <10,20,30>
(assert (= n 3))(assert (= (s 0) 10))(assert (= (s 1) 20))(assert (= (s 2) 30))
(check-sat)(pop)
(push)                                                  ; same literal, contradict slot 1
(assert (= n 3))(assert (= (s 1) 20))(assert (= (s 1) 99))
(check-sat)(pop)
