; prev_record_fields — external input pins
; last_results pinned to ⟨StringResult("done")⟩ so s=match last_results[0] is
; deterministic (an empty seq leaves last_results[0] free → Z3 picks arbitrarily,
; violating the determinism rule; see meta.json notes).
(assert (= last_results (seq.unit (StringResult "done"))))
