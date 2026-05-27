; async_stdin_line_read — golden witness
; state=LWait, world.stdin_line="hello" => state_next=LWait,
;   effects=⟨Println("echo: hello")⟩
(assert (= state_next LWait))
(assert (= effects (seq.unit (Println "echo: hello"))))
