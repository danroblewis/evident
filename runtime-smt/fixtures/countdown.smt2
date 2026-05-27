; Countdown FSM — the N2 worked fixture.
;
; State is a counter threaded count -> _count across ticks (init 3). While the
; previous count is positive it prints "tick"; once it reaches 0 it prints
; "done" and emits Exit(0). The run therefore prints tick/tick/tick/done and
; exits 0 in 4 ticks.
;
; Single self-contained file: embedded metadata (between @meta/@end, each line a
; `;` comment) then the named transition block. See ../FORMAT.md.
; @meta
; {
;   "fsms": [
;     { "name": "countdown",
;       "state": [{"prev":"_count","next":"count","sort":"Int","init":3}],
;       "effects": {"var":"effects"} }
;   ]
; }
; @end
; @transition countdown
(declare-datatypes ((Effect 0)) (((Println (msg String)) (Exit (code Int)))))
(declare-const _count Int)
(declare-const count Int)
(declare-const effects (Seq Effect))
(assert (= count (- _count 1)))
(assert (= effects
  (ite (> _count 0)
       (seq.unit (Println "tick"))
       (seq.++ (seq.unit (Println "done")) (seq.unit (Exit 0))))))
