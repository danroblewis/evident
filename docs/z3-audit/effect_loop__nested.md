# runtime/src/effect_loop/nested.rs — Z3-replaceability
**What it does:** Tier-3 blocking interpreter for `run(F, init)`: drives a nested FSM tick-by-tick (each tick is a Z3 solve via `query_with_pins_and_given`) until `halt = true`, capturing effects and returning the final state. Also validates `run` targets at load time.
**Criticality:** critical
**Verdict:** hot-path
**Confidence:** high
**How (if replaceable):** `run_nested_capturing` IS already the Z3 execution path — each step calls `rt.query_with_pins_and_given`, i.e., invokes Z3. The surrounding Rust code is the interpreter harness: the tick loop, state threading, halt detection, coercion, and effect collection. These are pure Rust orchestration around Z3 solves; they cannot themselves be replaced by a single Z3 solve because the composition is sequential (each tick's output seeds the next). The validate path is a pure AST-shape check (no solve needed). OFF-LIMITS for changes per task instructions.
**Change made:** none
