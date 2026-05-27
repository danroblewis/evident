; mode2_ordered_chain — golden witness
; state=MGo => state_next=MDone, dispatch order (order) =
;   ⟨Println("first"), Println("second"), Exit(0)⟩
(assert (= state_next MDone))
(assert (= order (seq.++ (seq.unit (Println "first")) (seq.++ (seq.unit (Println "second")) (seq.unit (Exit 0))))))
