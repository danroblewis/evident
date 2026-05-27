; Countdown FSM — decrements a counter each tick, emits a Tick effect, and
; raises `halt` when the count reaches 0. The N1/N2 worked fixture.
;
; Single self-contained file: embedded metadata (between @meta/@end, each line
; a `;` comment) followed by the named transition block. See ../FORMAT.md.
; @meta
; {
;   "fsms": [
;     { "name": "countdown",
;       "state": [{"prev":"_count","next":"count","sort":"Int","init":3}],
;       "effects": {"var":"effects"},
;       "halt": {"var":"halt"} }
;   ]
; }
; @end
; @transition countdown
(declare-datatypes ((Effect 0)) (((Println (msg String)) (Exit (code Int)) (Tick))))
(declare-const _count Int)
(declare-const count Int)
(declare-const effects (Seq Effect))
(declare-const halt Bool)
(assert (= count (- _count 1)))
(assert (= halt (<= count 0)))
(assert (= effects (seq.unit (as Tick Effect))))
