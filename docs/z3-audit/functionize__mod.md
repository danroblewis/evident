# runtime/src/functionize/mod.rs — Z3-replaceability
**What it does:** Module root for `functionize/`; declares the strategy submodules (cranelift, symbolic, llm, satisfier, glsl), re-exports the `Functionizer`/`CompiledFunction` traits from `core`, and provides `default()` which returns a boxed `CraneliftFunctionizer`.

**Criticality:** peripheral

**Verdict:** trivial

**Confidence:** high

**How (if replaceable):** Pure Rust module wiring and a one-line factory function. No logic to replace.

**Change made:** none
