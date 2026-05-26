# runtime/src/translate/inline/recursion.rs — Z3-replaceability
**What it does:** Bookkeeping for bounded recursive claim inlining: a `try_enter`/`exit_frame` depth counter (capped at `EVIDENT_MAX_INLINE_DEPTH`, default 64) that prevents runaway self-passthrough expansion, plus `isolate_helper_locals` which strips helper-internal locals from the cloned env on `ClaimCall` entry so recursive invocations get distinct Z3 constants.
**Criticality:** critical
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** This is guard/bookkeeping logic for the inliner that builds Z3 input — it prevents infinite unrolling of self-referential claims during compilation. It is part of the compile pipeline (termination control for the claim-expansion pass), not a decision problem Z3 could answer. Replacing it with a Z3 solve would be circular.
**Change made:** none
