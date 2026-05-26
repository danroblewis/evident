# runtime/src/ffi.rs — Z3-replaceability
**What it does:** dlopen/dlsym/libffi wrappers. Parses sig strings (`ret(args)` letter codes), marshals `FfiArg` variants into libffi `Arg`/`Cif`, calls through a `CodePtr`, returns `FfiReturn`. Owns the `HandleRegistry` (opaque u64 handle ↔ pointer map).
**Criticality:** critical
**Verdict:** not-a-CSP
**Confidence:** high
**How (if replaceable):** Tier-0 kernel per the audit invariants. This IS the runtime bridge to C: it wraps unsafe pointer arithmetic, libffi CIF construction, and dynamic library loading. No constraint search is involved. A Z3 solve cannot call into arbitrary C symbols or manage a pointer handle registry.
**Change made:** none
