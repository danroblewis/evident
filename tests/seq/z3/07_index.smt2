;; C7 indexing xs[i]. Read slot 1 of a pinned literal.
;; expect: sat unsat
(declare-fun s (Int) Int)
(define-fun lit () Bool (and (= (s 0) 10)(= (s 1) 20)(= (s 2) 30)))
(push)(assert lit)(assert (= (s 1) 20))(check-sat)(pop)  ; positive
(push)(assert lit)(assert (= (s 1) 99))(check-sat)(pop)  ; negative
