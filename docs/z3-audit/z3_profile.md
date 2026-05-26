# runtime/src/z3_profile.rs — Z3-replaceability
**What it does:** Optional Z3 profiling: aggregates solver statistics per-check and per-claim into a thread-local `Z3ProfileStats` (gated on `EVIDENT_PROFILE_Z3=1`), prints a formatted summary, extracts UNSAT cores, and enables axiom-profiler trace output. All off by default.
**Criticality:** does-little
**Verdict:** not-a-CSP
**Confidence:** high
**How (if replaceable):** Pure observability/instrumentation code — reads Z3 solver statistics after `check()` calls and formats them. No constraint structure; no satisfiability question. Z3 cannot instrument itself via a solve. The module is entirely opt-in diagnostic tooling with no effect on normal execution.
**Change made:** none
