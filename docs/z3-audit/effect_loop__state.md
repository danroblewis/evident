# runtime/src/effect_loop/state.rs — Z3-replaceability
**What it does:** Two helpers for the scheduler: `encode_state_value` converts a `Value::Enum` to a `z3::ast::Datatype` (for pinning state as a Z3 constant on the next tick); `seed_state_with_arg` wraps a spawn argument into a first-variant enum value for newly spawned FSMs. Also `model_matches_value` for halt-sentinel detection (legacy, currently unused in the main scheduler path).
**Criticality:** critical
**Verdict:** not-a-CSP
**Confidence:** high
**How (if replaceable):** These are pure encoding/marshaling utilities — converting Rust `Value` trees to Z3 AST nodes. They don't solve anything; they prepare inputs for Z3. There is no constraint problem to delegate. `encode_state_value` is called on the hot path (every tick, every FSM with state). A solve cannot replace encoding.
**Change made:** none
