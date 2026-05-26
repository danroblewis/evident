# runtime/src/runtime/lenient.rs — Z3-replaceability
**What it does:** A 25-line RAII guard (`LenientGuard`) that sets `EVIDENT_LENIENT=1` while alive and restores the prior environment variable state on drop. Lets the functionizer skip untranslatable body items instead of fatal-exiting.
**Criticality:** peripheral (used at specific call sites to enable lenient mode temporarily)
**Verdict:** trivial
**Confidence:** high
**How (if replaceable):** Too small to be worth a solve — it is a pure env-var RAII wrapper with no algorithmic content. Not a constraint-satisfaction problem.
**Change made:** none
