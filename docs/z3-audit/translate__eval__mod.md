# runtime/src/translate/eval/mod.rs — Z3-replaceability
**What it does:** Module glue for `translate/eval/`. Declares the six submodules (`solver`, `decode`, `cached`, `extra`, `core`, `decompose`), re-exports their public symbols, and houses the canonical `evaluate` function (the one-shot full solve: declare vars, pin givens, assert constraints, check, extract model).
**Criticality:** critical (re-exports `evaluate`, `build_cache`, `run_cached`, etc. — everything the rest of the runtime calls into)
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** `evaluate` in this file calls `solver.check()` and walks the Z3 model — it is the solve-driver. The re-export structure is module glue. Nothing here is a standalone algorithm that could be expressed as an Evident constraint; the entire file drives Z3.
**Change made:** none
