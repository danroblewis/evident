; async_signal_exit — golden witness
; state=SRunning, world.signal_received=1 => state_next=SHalted,
;   effects=⟨Println("interrupted, exiting"), Exit(130)⟩
(assert (= state_next SHalted))
(assert (= effects (seq.++ (seq.unit (Println "interrupted, exiting")) (seq.unit (Exit 130)))))
