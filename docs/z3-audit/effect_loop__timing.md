# runtime/src/effect_loop/timing.rs — Z3-replaceability
**What it does:** Two `eprintln!`-based timing summary formatters, gated by `EVIDENT_LOOP_TIMING`. No logic — purely formats and prints elapsed time / per-FSM tick stats.
**Criticality:** peripheral
**Verdict:** trivial
**Confidence:** high
**How (if replaceable):** Diagnostic output only; no computation, no constraint content. Not a CSP in any sense. A Z3 solve over timing data would be absurd.
**Change made:** none
