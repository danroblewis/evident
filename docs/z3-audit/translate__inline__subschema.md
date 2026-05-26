# runtime/src/translate/inline/subschema.rs — Z3-replaceability
**What it does:** Handles two subclaim-of-type invocation forms: `inline_subschema_call` for `recv.subclaim(args)` (mirrors receiver fields as bare names so the subclaim body resolves, then inlines body constraints), and `inline_forall_subschema` for `∀ vars ∈ range : recv.subclaim(args)` (statically unrolls the range and dispatches each iteration, working around `translate_bool` lacking solver access).
**Criticality:** critical
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** This file expands subclaim invocations into Z3 assertions, including the field-rebinding and static `∀` unrolling that make subclaims work inside quantifiers. It is part of the claim-compilation pipeline that produces Z3 input; the expansion must happen before Z3 solves. Replacing it with a Z3 solve would be circular.
**Change made:** none
