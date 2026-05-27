; async_stdin_line_read — transition relation for one tick of fsm `line_reader`
; FSM source: runtime-contract/fixtures/async_stdin_line_read/source.ev
; Derived from: source.ev  claim sat_echoes_stdin_line
;
; Concatenate: problem.smt2 ++ prev.smt2 ++ inputs.smt2 ; then (check-sat).
; effects_in_smt: true.
;
; Captures the READ of the async-injected world field `stdin_line` (written by
; StdinSource). The blocking read(2) injection is external and NOT modeled here.

(declare-datatypes
  ((LState 0) (Effect 0))
  (((LWait) (LDone))
   ((NoEffect)
    (Print    (Print_0    String))
    (Println  (Println_0  String))
    (Exit     (Exit_0     Int))
    (IntToStr (IntToStr_0 Int)))))

(declare-const |world.stdin_line| String)
(declare-const state      LState)
(declare-const state_next LState)
(declare-const effects    (Seq Effect))

; state_next = (stdin_line = "quit" ? LDone : LWait)
(assert (= state_next
  (ite (= |world.stdin_line| "quit") LDone LWait)))

; effects = (stdin_line = "quit" ? ⟨Println("bye"), Exit(0)⟩
;                                 : ⟨Println("echo: " ++ stdin_line)⟩)
(assert (= effects
  (ite (= |world.stdin_line| "quit")
       (seq.++ (seq.unit (Println "bye")) (seq.unit (Exit 0)))
       (seq.unit (Println (str.++ "echo: " |world.stdin_line|))))))
