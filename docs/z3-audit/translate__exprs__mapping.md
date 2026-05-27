# runtime/src/translate/exprs/mapping.rs — Z3-replaceability
**What it does:** Resolves claim-call argument expressions to `(env-key, Var<'ctx>)` binding lists used when inlining a sub-claim's body into an outer translation environment. Handles identifier passthrough, record literals (with tuple coercion), `seq[i]` composite expansion, field-chain drilling into nested `Seq(Composite)`, and scalar leaf coercion.
**Criticality:** critical
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** This file is a sub-pass of the AST→Z3 translation pipeline; its output is a set of Z3 AST variable bindings that subsequent translation steps consume to inline claim bodies. Replacing it with a Z3 solve would require those bindings to already exist — circular. The work it does (structural field navigation into Z3 datatypes) has no meaningful expression as a Z3 constraint problem.
**Change made:** none
