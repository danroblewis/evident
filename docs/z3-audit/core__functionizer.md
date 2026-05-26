# runtime/src/core/functionizer.rs — Z3-replaceability

**What it does:** Defines the `Functionizer` trait (compile a `Z3Program` to a callable artifact) and `CompiledFunction` trait (call the artifact with given bindings, or return `None` to fall through to Z3). These are the strategy-pattern interfaces; concrete implementations live under `crate::functionize::*`.

**Criticality:** critical

**Verdict:** not-a-CSP

**Confidence:** high

**How (if replaceable):** Not applicable. This file defines Rust traits — abstract interfaces for the JIT/AOT compilation strategy. It contains no algorithm, no data, and no logic. It is the seam between the IR (`Z3Program`) and the compilation backends (Cranelift, satisfier, symbolic, etc.). Z3 is what the `CompiledFunction` fallback invokes; you cannot replace the interface definition for that fallback with Z3 itself.

**Change made:** none
