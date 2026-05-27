; inputs.smt2 — external input pins for world_reader_consumer
; world.n = 7 (the value the reader observes from the shared world)
; last_results is empty (no prior effects to feed back)

(assert (= |world.n| 7))
(assert (= last_results (as seq.empty (Seq Result))))
