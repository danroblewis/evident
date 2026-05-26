# runtime/src/value_builders.rs — Z3-replaceability
**What it does:** Provides `#[no_mangle] extern "C"` callback functions (`ev_set_int`, `ev_set_bool`, `ev_set_str`, `ev_set_enum_*`, etc.) called by Cranelift JIT-compiled code to construct heap-allocated `Value` variants; also `ev_init_slot` for initializing uninitialized stack slots.
**Criticality:** critical
**Verdict:** not-a-CSP
**Confidence:** high
**How (if replaceable):** These are ABI-level C callbacks wired into the JIT code-generation path. They perform pointer writes into pre-allocated `Value` slots with `unsafe` Rust. There is no constraint-satisfaction problem here; they are the runtime's FFI glue between native code and Rust's heap. Z3 cannot provide C ABI callbacks.
**Change made:** none
