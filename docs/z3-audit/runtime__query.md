# runtime/src/runtime/query.rs — Z3-replaceability
**What it does:** The primary query dispatch layer: `query` and `query_cached` are the public entry points that take a schema name + given bindings and return a `QueryResult`. Implements the per-component JIT fast path (decompose simplified assertions via union-find, compile each component with Cranelift, fall through to scoped Z3 solves for uncompilable components), a cross-tick value cache, parallel slow-part solving, and the tier-1 affine-unroll accelerator for `run(F, init)`.
**Criticality:** critical (every solve — both load-time and per-tick — flows through here)
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** This file IS the solve entrypoint — it orchestrates building a Z3 solver, translating the schema, running the check, and extracting the model. Replacing its job with a Z3 solve would be self-referential (you need this code to call Z3). The decompose/compile/cache logic is optimization harness around the solve, not a CSP itself. The union-find component decomposition algorithm (`decompose_simplified`) is a graph-connectivity computation that could in principle be expressed as a constraint, but it runs inside the harness that builds the Z3 query, so it is structurally circular.
**Change made:** none
