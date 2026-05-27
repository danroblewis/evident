; inputs.smt2 — external input pins for world_writer_producer
; world.n is read-free for the producer's next_n computation (PTick(k) => k, not world.n).
; We pin world.n = 0 for determinism, though it does not affect the golden outputs.
; last_results is empty (no prior effects to feed back).

(assert (= |world.n| 0))
(assert (= last_results (as seq.empty (Seq Result))))
