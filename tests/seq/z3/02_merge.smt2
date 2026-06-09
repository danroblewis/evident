;; A2 overlapping-chain merge. ch1: a<b<c<f<g, ch2: c<d<e<f, shared c,f.
;; Merge is just the union of the order constraints over named ints.
;; expect: sat unsat
(declare-const a Int)(declare-const b Int)(declare-const c Int)
(declare-const d Int)(declare-const e Int)(declare-const f Int)(declare-const g Int)
(push)                                  ; positive: merged a..g exists
(assert (< a b))(assert (< b c))(assert (< c f))(assert (< f g))
(assert (< c d))(assert (< d e))(assert (< e f))
(check-sat)
(pop)
(push)                                  ; negative: conflicting overlap f<c contradicts c<f
(assert (< c f))                        ; from ch1
(assert (< f d))(assert (< d c))        ; from a conflicting ch2: f<d<c
(check-sat)
(pop)
