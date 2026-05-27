; Ping-pong FSM — state toggles 0 <-> 1 forever (no halt; relies on the
; driver's max-ticks cap). It revisits only TWO distinct previous-state values,
; so a cached run (N4a) solves Z3 exactly twice and serves every later tick from
; the transition cache. The N4a worked fixture.
; @meta
; {
;   "fsms": [
;     { "name": "pingpong",
;       "state": [{"prev":"_s","next":"s","sort":"Int","init":0}] }
;   ]
; }
; @end
; @transition pingpong
(declare-const _s Int)
(declare-const s Int)
(assert (= s (- 1 _s)))
