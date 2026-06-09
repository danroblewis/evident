;; D13 permutation / all-distinct of 0..N-1. Pigeonhole makes the
;; over-constrained case unsat — decidable, no theory of sequences.
;; expect: sat unsat
(declare-fun s (Int) Int)
(define-fun distinctN ((N Int)) Bool
  (and (forall ((i Int)) (=> (and (<= 0 i)(< i N)) (and (<= 0 (s i)) (< (s i) N))))
       (forall ((i Int)(j Int)) (=> (and (<= 0 i)(< i N)(<= 0 j)(< j N)(not (= i j))) (not (= (s i) (s j)))))))
(push)(assert (distinctN 3))(assert (= (s 0) 2))(check-sat)(pop)   ; 3 distinct in {0,1,2}
(push)                                                             ; 4 distinct values all in {0,1,2}: pigeonhole
(assert (forall ((i Int)) (=> (and (<= 0 i)(< i 4)) (and (<= 0 (s i)) (< (s i) 3)))))
(assert (forall ((i Int)(j Int)) (=> (and (<= 0 i)(< i 4)(<= 0 j)(< j 4)(not (= i j))) (not (= (s i) (s j))))))
(check-sat)(pop)
