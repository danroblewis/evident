; N0 floor fixture — a hardcoded scalar problem with a forced model.
; Solve with: cargo run --manifest-path runtime-smt/Cargo.toml -- solve runtime-smt/fixtures/n0_floor.smt2
(declare-const n Int)
(declare-const ok Bool)
(assert (= n 7))
(assert (= ok (> n 5)))
