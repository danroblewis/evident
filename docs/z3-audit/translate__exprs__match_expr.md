# runtime/src/translate/exprs/match_expr.rs — Z3-replaceability
**What it does:** Compiles Evident `match scrutinee / Ctor(b) ⇒ body / _ ⇒ fallback` expressions to nested Z3 `ite` (if-then-else) chains. `translate_match_arms` resolves the scrutinee to a Z3 Datatype, walks each arm's pattern to produce recognizer testers and payload bindings, then `fold_arms_to_ite` folds the compiled arms bottom-up into a single Z3 expression.
**Criticality:** critical
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** This file is the match-lowering phase of the AST→Z3 compilation pipeline; it produces Z3 ITE AST nodes. Those nodes are the Z3 input — you cannot use a Z3 solve to replace the code that constructs the Z3 input. No standalone decidable property is computed here.
**Change made:** none
