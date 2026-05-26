# runtime/src/translate/eval/cached.rs — Z3-replaceability
**What it does:** Implements the per-tick cached query path: `build_cache` translates a schema body once into a persistent Z3 solver (declarations + constraints), and `run_cached` reuses it per tick via push/assert-givens/check/pop. `sample_cached_inner` is the n-models sampler using blocking clauses.
**Criticality:** critical (this is the innermost per-tick hot path for all FSM queries in the scheduler loop)
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** `run_cached` IS the code that calls Z3 — it pushes givens, calls `solver.check()`, extracts the model, and pops. Replacing it with a Z3 solve would be replacing the call to Z3 with a call to Z3, which is circular. `build_cache` similarly constructs and populates the Z3 solver object. The entire file is the solve-driver layer; there is no higher-level algorithm here that could be expressed as a constraint.
**Change made:** none
