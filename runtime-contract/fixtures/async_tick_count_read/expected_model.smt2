; async_tick_count_read — golden witness
; state=TWatch, world.tick_count=5 => state_next=TWatch, effects=⟨IntToStr(5)⟩
(assert (= state_next TWatch))
(assert (= effects (seq.unit (IntToStr 5))))
