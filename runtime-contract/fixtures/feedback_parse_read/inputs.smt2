; feedback_parse_read — external input pins
; Pins last_results = ⟨IntResult(42), ErrorResult("invalid digit")⟩

(assert (= last_results
  (seq.++ (seq.unit (IntResult 42))
          (seq.unit (ErrorResult "invalid digit")))))
