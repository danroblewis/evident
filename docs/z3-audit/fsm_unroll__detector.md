# runtime/src/fsm_unroll/detector.rs — Z3-replaceability
**What it does:** Classifies an FSM body as affine (log-unroll collapses) vs. branching (refuses) by measuring the Z3 AST node-count ratio after each doubling step, using F^8 as the probe depth with a 1.5 ratio threshold.

**Criticality:** peripheral

**Verdict:** not-a-CSP

**Confidence:** high

**How (if replaceable):** The classification is a purely numeric threshold on an already-computed ratio (`last_ratio <= 1.5`). There are only two lines of real logic: `classify` and `count_nodes`. A Z3 solve would buy nothing — there is no constraint to satisfy; this is a heuristic measurement of symbolic-complexity growth. The decision is structural (does the AST shrink when folded?) not a satisfiability question.

**Change made:** none
