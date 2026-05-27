; prev_increment — previous-state pins (subsequent tick)
; is_first_tick = false → _count is pinned from prev_values

(assert (= is_first_tick false))
(assert (= _count 7))
(assert (= state Counting))
