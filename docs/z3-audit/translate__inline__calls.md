# runtime/src/translate/inline/calls.rs — Z3-replaceability
**What it does:** Inlines four flavors of claim invocation into the Z3 solver: tuple-in-claim (`(args) ∈ ClaimName`), positional call (`claim(args)`), guarded claim (`cond ⇒ ClaimName`), and explicit-mapping call (`ClaimName(slot ↦ val)`). For each flavor it clones the environment, isolates helper locals, resolves arg bindings, declares fresh Z3 constants for uncovered slots, then recurses into `inline_body_items_guarded` to assert the claim's body constraints.
**Criticality:** critical
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** This file IS the mechanism that expands claim calls into Z3 constraints — it builds the flattened constraint set that Z3 then solves. Replacing it with a Z3 solve would require Z3 to receive unexpanded claim references, which it cannot resolve; the inliner must run first. This is core claim-compilation logic, not a decidable property.
**Change made:** none
