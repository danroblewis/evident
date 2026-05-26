# runtime/src/runtime/analysis.rs — Z3-replaceability
**What it does:** Exposes three diagnostic query methods on `EvidentRuntime`: structural decomposition of a claim into independent sub-models, component classification (function-shaped vs search-shaped via 2-copy uniqueness check), and `query_with_core` (SAT/UNSAT + unsat-core body indices). All three delegate immediately into `crate::translate` and `crate::translate::eval`.
**Criticality:** peripheral (diagnostic/test path — `query_with_core` used by `evident test`; decomposition/classify used by analysis commands)
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** These methods ARE the entry point that invokes Z3 solves. `analyze_decomposition` and `classify_components` call into `translate::analyze_decomposition` / `classify_components`, which build Z3 contexts and run solvers. `query_with_core` runs a constraint solve and extracts the unsat core. Replacing this file's orchestration with a Z3 solve would be circular — you need these methods to run any Z3 query at all. The methods themselves are thin glue (schema lookup + env-var → delegate), not algorithms.
**Change made:** none
