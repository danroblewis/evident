# runtime/src/translate/exprs/bool.rs — Z3-replaceability
**What it does:** Translates Evident boolean-sort expressions (And/Or/Implies, Eq/Neq/comparisons, contains, distinct, set membership, In-expr, Matches/Forall/Exists, Ternary, Match, Index) into Z3 `Bool<'ctx>` AST nodes by dispatching to helpers in sibling modules. It is the central boolean-expression compiler in the AST→Z3 translation pipeline.
**Criticality:** critical
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** This file IS the front-end that constructs the Z3 boolean terms that a solve subsequently runs on. Replacing it with a Z3 solve would require Z3 to already have the terms — a direct bootstrap cycle. There is no standalone algorithm here that could be expressed as an Evident constraint; the function's job is to produce the constraint representation itself.
**Change made:** none
