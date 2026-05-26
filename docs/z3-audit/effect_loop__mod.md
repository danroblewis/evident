# runtime/src/effect_loop/mod.rs — Z3-replaceability
**What it does:** Top-level entry point for `effect-run`: discovers FSMs, installs event-source plugins and FTI bridges, builds transitive access sets, enforces the single-owner disjointness invariant (`check_single_owner`), then delegates to the subscription-driven scheduler. Also re-exports the module's public API.
**Criticality:** critical
**Verdict:** hot-path
**Confidence:** high
**How (if replaceable):** The orchestration itself is pure control flow (Tier-4). `check_single_owner` is the canonical documented mode-2 candidate explicitly kept in Rust: a SAT/UNSAT answer cannot name the conflicting writer pair, so a Z3 solve would be strictly worse (see the doc comment in the source and `docs/design/self-hosting-inventory.md`). The plugin-install and FTI-wiring loops are imperative setup with no constraint content. Nothing here is a CSP.
**Change made:** none
