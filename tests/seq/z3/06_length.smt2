;; C6 length #xs. A bounded literal has a fixed length.
;; expect: sat unsat
(declare-const n Int)
(push)(assert (= n 4))(check-sat)(pop)              ; positive
(push)(assert (= n 4))(assert (= n 5))(check-sat)(pop) ; negative: a seq has one length
