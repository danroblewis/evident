# runtime/src/parser/exprs.rs — Z3-replaceability
**What it does:** Implements the precedence-climbing expression parser: quantifiers (`∀`/`∃`), implication (`⇒`), ternary (`?:`), boolean operators (`∧`/`∨`), comparison operators with chained-comparison support (`20 ≤ x ≤ 740`), arithmetic (`+`/`-`/`*`/`/`/`++`), unary operators (`¬`/`-`/`#`), and postfix indexing/field-access. Produces nested `Expr` AST nodes.
**Criticality:** critical
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** This is a pure token-stream → AST transform implementing operator-precedence grammar rules. The precedence hierarchy, chained-comparison desugaring (AND-combining pairwise), and block-form handling (`⇒\n    body…`) are all control flow over token positions — not constraint satisfaction. A Z3 solve needs `Expr` nodes to exist before it can reason; this file is what creates those nodes. Replacement is circular.
**Change made:** none
