; mode2_ordered_chain — transition relation for one tick of fsm `mode2_demo`
; FSM source: runtime-contract/fixtures/mode2_ordered_chain/source.ev
; Derived from: source.ev  claim sat_dispatches_in_order
;
; Concatenate: problem.smt2 ++ prev.smt2 ++ inputs.smt2 ; then (check-sat).
; effects_in_smt: true ; dispatch_mode: 2.
;
; MODE 2: the FSM declares no `effects` slot; the runtime scrapes its Effect
; bindings and TOPOSORTS them by the `order = ⟨a,b,c⟩` ordering-edge declaration
; (edges a→b→c) before dispatching. The toposort result [a,b,c] is a TOTAL order
; (no tie-break / EVIDENT_DISPATCH_SEED dependence). This portable capture
; encodes that resulting dispatch order as the `order` Seq, which both SMT
; engines decode. The CurrentRuntime gate witnesses the SAME order by running the
; real toposort (collect_tick_effects, primary_var=None) — confirmed independently
; by `evident effect-run` (stdout: first / second, exit 0).

(declare-datatypes
  ((MState 0) (Effect 0))
  (((MGo) (MDone))
   ((NoEffect)
    (Print    (Print_0    String))
    (Println  (Println_0  String))
    (Exit     (Exit_0     Int))
    (IntToStr (IntToStr_0 Int)))))

(declare-const state      MState)
(declare-const state_next MState)
; `order` holds the toposorted dispatch order (the ordering declaration value).
(declare-const order      (Seq Effect))

; state_next = match state { MGo => MDone ; MDone => MDone }
(assert (= state_next (ite (is-MGo state) MDone MDone)))

; order = the dispatch order the mode-2 toposort produces: ⟨a, b, c⟩ resolved =
;   ⟨Println("first"), Println("second"), Exit(0)⟩
(assert (= order
  (seq.++ (seq.unit (Println "first"))
          (seq.++ (seq.unit (Println "second")) (seq.unit (Exit 0))))))
