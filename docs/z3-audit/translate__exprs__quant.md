# runtime/src/translate/exprs/quant.rs — Z3-replaceability
**What it does:** Unrolls Evident `∀`/`∃` quantifiers over integer ranges, primitive and composite `Seq` variables, `coindexed(...)` parallel-sequence zips, `edges(...)` adjacent-pair iteration, and `Set` subset shortcut into finite Z3 conjunctions or disjunctions. Each iteration index is substituted into the body and the resulting Bool terms are AND-ed (∀) or OR-ed (∃).
**Criticality:** critical
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** This file is the quantifier-lowering pass of the AST→Z3 compilation pipeline; it produces Z3 `Bool::and`/`Bool::or` AST nodes that form the constraint input to the solver. Using a Z3 solve to replace the code that constructs Z3 expressions would be circular. The bounded unrolling is a compile-time transformation, not a separate decidable property.
**Change made:** none
