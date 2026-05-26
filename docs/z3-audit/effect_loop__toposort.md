# runtime/src/effect_loop/toposort.rs — Z3-replaceability
**What it does:** Thin wrapper: `evident_toposort` delegates to `crate::portable::toposort::toposort` (the self-hosted Evident `ToposortRanks` claim, already a Z3 solve). Provides the process-wide `DISPATCH_ORDER_CACHE` memo, `cycle_recovery` (fallback to input order on UNSAT), and `resolve_synthetic_names_to_effects` (name→Effect lookup after sort).
**Criticality:** critical (on the tick-0 path; cached thereafter)
**Verdict:** replaceable-as-group(effect_loop/toposort.rs, portable/toposort.rs)
**Confidence:** high
**How (if replaceable):** The core toposort solve IS already in Z3 (via the self-hosted `ToposortRanks` Evident claim in `portable/toposort.rs`). The PORT-toposort session documented that the naive domain-typed `Toposort<String>` path was 13–42s tick-0 (1000× slower); the int-rank encoding brought it to ~19ms. The current file is already the Z3-solve path with memoization. `cycle_recovery` and `resolve_synthetic_names_to_effects` are tiny pure-Rust utilities that complement the solve and cannot themselves be replaced by one. No further Z3-ification opportunity exists in this file that isn't already exploited.
**Change made:** none
