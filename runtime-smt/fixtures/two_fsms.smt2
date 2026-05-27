; Two FSMs sharing world state — the N3 worked fixture (writer/reader).
;
; `producer` owns a private counter (prev _p, next p, init 3) and writes the
; shared world var `n` = _p each tick (so n goes 3,2,1,0). `consumer` reads the
; SAME-tick `n` (writer-first scheduling pins producer's write as consumer's
; given) and prints "consumed" while n is positive. When the producer's prev
; count hits 0 it prints "producer done" and Exit(0).
;
; Run prints consumed/consumed/consumed/producer done and exits 0 in 4 ticks —
; byte-identical to crosscheck/two_fsms.ev on the legacy runtime.
; @meta
; {
;   "world": [{"name":"n","sort":"Int","init":0}],
;   "fsms": [
;     { "name":"producer",
;       "state":[{"prev":"_p","next":"p","sort":"Int","init":3}],
;       "world_writes":["n"],
;       "effects":{"var":"effects"} },
;     { "name":"consumer",
;       "world_reads":["n"],
;       "effects":{"var":"effects"} }
;   ]
; }
; @end
; @transition producer
(declare-datatypes ((Effect 0)) (((Println (msg String)) (Exit (code Int)))))
(declare-const _p Int)
(declare-const p Int)
(declare-const n Int)
(declare-const effects (Seq Effect))
(assert (= p (ite (> _p 0) (- _p 1) 0)))
(assert (= n _p))
(assert (= effects (ite (<= _p 0)
                        (seq.++ (seq.unit (Println "producer done")) (seq.unit (Exit 0)))
                        (as seq.empty (Seq Effect)))))
; @transition consumer
(declare-datatypes ((Effect 0)) (((Println (msg String)) (Exit (code Int)))))
(declare-const n Int)
(declare-const effects (Seq Effect))
(assert (= effects (ite (> n 0)
                        (seq.unit (Println "consumed"))
                        (as seq.empty (Seq Effect)))))
