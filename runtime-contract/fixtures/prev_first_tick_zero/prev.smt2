; prev_first_tick_zero — previous-state pins (tick 0)
; is_first_tick = true → no _count pin needed (is_first branch taken)

(assert (= is_first_tick true))
(assert (= state Counting))
