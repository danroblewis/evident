; world_writer_producer — transition relation for one tick of fsm `producer`
; FSM source: runtime-contract/fixtures/world_writer_producer/source.ev
; Derived from: examples/test_09_two_fsms.ev  claim sat_producer_writes_n
;
; Concatenate:  problem.smt2 ++ prev.smt2 ++ inputs.smt2
; Then append:  (check-sat) / (get-model) / uniqueness assertions
; None of these files contain (check-sat).

; ── Datatype declarations ─────────────────────────────────────────────────────

(declare-datatypes
  ((PState 0) (CState 0) (Effect 0) (Result 0))
  (((PStart) (PTick (PTick_0 Int)) (PEnd))
   ((CWait) (CFormat) (CEnd))
   ((NoEffect)
    (Print    (Print_0    String))
    (Println  (Println_0  String))
    (ReadLine)
    (Time)
    (Exit     (Exit_0     Int))
    (ParseInt (ParseInt_0 String))
    (IntToStr (IntToStr_0 Int)))
   ((NoResult)
    (IntResult    (IntResult_0    Int))
    (StringResult (StringResult_0 String))
    (BoolResult   (BoolResult_0   Bool))
    (RealResult   (RealResult_0   Real))
    (HandleResult (HandleResult_0 Int))
    (ErrorResult  (ErrorResult_0  String)))))

; ── World fields (flattened as scalar consts) ──────────────────────────────────
; Pipe-quoted to preserve dotted names

(declare-const |world.n|      Int)
(declare-const |world_next.n| Int)

; ── Infrastructure constants ───────────────────────────────────────────────────

(declare-const state        PState)
(declare-const state_next   PState)
(declare-const effects      (Seq Effect))
(declare-const last_results (Seq Result))
(declare-const next_n       Int)

; ── Transition constraints ─────────────────────────────────────────────────────

; state_next = match state
;   PStart   => PTick(3)
;   PTick(k) => (k <= 1 ? PEnd : PTick(k - 1))
;   PEnd     => PEnd
(assert (= state_next
  (ite (is-PStart state) (PTick 3)
  (ite (is-PTick  state) (ite (<= (PTick_0 state) 1) PEnd (PTick (- (PTick_0 state) 1)))
                         PEnd))))

; next_n = match state
;   PStart   => 3
;   PTick(k) => k
;   PEnd     => 0
(assert (= next_n
  (ite (is-PStart state) 3
  (ite (is-PTick  state) (PTick_0 state)
                         0))))

; world_next.n = next_n
(assert (= |world_next.n| next_n))

; effects = match state
;   PEnd => [Println("producer done"), Exit(0)]
;   _    => []
(assert (= effects
  (ite (is-PEnd state)
       (seq.++ (seq.unit (Println "producer done"))
               (seq.unit (Exit 0)))
       (as seq.empty (Seq Effect)))))
