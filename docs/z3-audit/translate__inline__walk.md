# runtime/src/translate/inline/walk.rs — Z3-replaceability
**What it does:** The main dispatch loop for body-item inlining: `inline_body_items_guarded` iterates `BodyItem` variants and routes each to the appropriate sub-handler (membership declaration, constraint assertion, passthrough expansion, ClaimCall inlining, subschema dispatch, guarded-claim wrapping, `∀`-with-subclaim static unrolling). The two public entry points `inline_body_items` and `inline_body_items_tracked` are thin wrappers that set the guard/tracker arguments.
**Criticality:** critical
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** This is the top-level driver of the claim-inliner — the pass that converts the Evident AST body into a flat set of Z3 assertions before Z3 solves. It is the compile front-end dispatcher; its output IS the Z3 problem input. Replacing it with a Z3 solve would be circular.
**Change made:** none
