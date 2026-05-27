; async_tick_count_read — transition relation for one tick of fsm `ticker`
; FSM source: runtime-contract/fixtures/async_tick_count_read/source.ev
; Derived from: source.ev  claim sat_emits_tick_count
;
; Concatenate: problem.smt2 ++ prev.smt2 ++ inputs.smt2 ; then (check-sat).
; effects_in_smt: true.
;
; Captures the READ of the async-injected world field `tick_count` (written by
; FrameTimer). The wall-clock injection is external and NOT modeled here.

(declare-datatypes
  ((TState 0) (Effect 0))
  (((TWatch) (TStop))
   ((NoEffect)
    (Print    (Print_0    String))
    (Println  (Println_0  String))
    (Exit     (Exit_0     Int))
    (IntToStr (IntToStr_0 Int)))))

(declare-const |world.tick_count| Int)
(declare-const state      TState)
(declare-const state_next TState)
(declare-const effects    (Seq Effect))

; state_next = (tick_count >= 10 ? TStop : TWatch)
(assert (= state_next
  (ite (>= |world.tick_count| 10) TStop TWatch)))

; effects = (tick_count >= 10 ? ⟨Exit(0)⟩ : ⟨IntToStr(tick_count)⟩)
(assert (= effects
  (ite (>= |world.tick_count| 10)
       (seq.unit (Exit 0))
       (seq.unit (IntToStr |world.tick_count|)))))
