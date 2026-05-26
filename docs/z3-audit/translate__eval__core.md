# runtime/src/translate/eval/core.rs — Z3-replaceability
**What it does:** The UNSAT-core variant of `evaluate`: tags each body-item assertion with a tracker boolean, calls `solver.check_assumptions(&trackers)`, and on UNSAT extracts `get_unsat_core()` to map conflicting assertions back to source body-item indices.
**Criticality:** peripheral (diagnostic/debugging path, not on the normal per-tick solve path)
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** This file IS the code that invokes Z3's UNSAT-core extraction via `check_assumptions` and `get_unsat_core`. It is post-processing the result of a Z3 solve to produce diagnostic indices. Expressing this as a Z3 constraint would be circular — you would need Z3 to run in order to find out which of Z3's own constraints are in conflict.
**Change made:** none
