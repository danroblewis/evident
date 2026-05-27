; async_signal_exit — transition relation for one tick of fsm `sig_watcher`
; FSM source: runtime-contract/fixtures/async_signal_exit/source.ev
; Derived from: source.ev  claim sat_exits_on_signal
;
; Concatenate: problem.smt2 ++ prev.smt2 ++ inputs.smt2 ; then (check-sat).
; effects_in_smt: true.
;
; Captures the READ of the async-injected world field `signal_received`
; (written by SigintSource). The injection itself (signal delivery) is external
; and is NOT modeled here; this is the deterministic reader transition.

(declare-datatypes
  ((SigState 0) (Effect 0))
  (((SRunning) (SHalted))
   ((NoEffect)
    (Print    (Print_0    String))
    (Println  (Println_0  String))
    (Exit     (Exit_0     Int))
    (IntToStr (IntToStr_0 Int)))))

; World field (flattened scalar const; pipe-quoted dotted name)
(declare-const |world.signal_received| Int)

(declare-const state      SigState)
(declare-const state_next SigState)
(declare-const effects    (Seq Effect))

; state_next = (signal_received > 0 ? SHalted : SRunning)
(assert (= state_next
  (ite (> |world.signal_received| 0) SHalted SRunning)))

; effects = (signal_received > 0 ? ⟨Println("interrupted, exiting"), Exit(130)⟩ : ⟨⟩)
(assert (= effects
  (ite (> |world.signal_received| 0)
       (seq.++ (seq.unit (Println "interrupted, exiting")) (seq.unit (Exit 130)))
       (as seq.empty (Seq Effect)))))
