# runtime/src/fsm_unroll/mod.rs — Z3-replaceability
**What it does:** Module root for `fsm_unroll/`; declares the two submodules (`compose`, `detector`) and re-exports the three public symbols (`assert_halts_within`, `collapse_run`, `TierOneRun`).

**Criticality:** peripheral

**Verdict:** trivial

**Confidence:** high

**How (if replaceable):** Pure Rust module wiring with no logic. Nothing to replace.

**Change made:** none
