;; A3 toposort. pos is a permutation of 0..N-1 (output ranks); each edge
;; (u,v) requires pos[u] < pos[v]. Bounded: N=4 nodes, pos as a function.
;; expect: sat unsat
(define-fun N () Int 4)
(declare-fun pos (Int) Int)
(define-fun ranged () Bool (forall ((i Int)) (=> (and (<= 0 i) (< i N)) (and (<= 0 (pos i)) (< (pos i) N)))))
(define-fun perm () Bool (forall ((i Int) (j Int))
  (=> (and (<= 0 i) (< i N) (<= 0 j) (< j N) (not (= i j))) (not (= (pos i) (pos j))))))
(push)                                  ; positive: DAG 0->2, 1->3 has a topo order
(assert ranged)(assert perm)
(assert (< (pos 0) (pos 2)))(assert (< (pos 1) (pos 3)))
(check-sat)
(pop)
(push)                                  ; negative: cycle 0->1, 1->0 has none
(assert ranged)(assert perm)
(assert (< (pos 0) (pos 1)))(assert (< (pos 1) (pos 0)))
(check-sat)
(pop)
