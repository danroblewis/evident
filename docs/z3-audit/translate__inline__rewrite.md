# runtime/src/translate/inline/rewrite.rs — Z3-replaceability
**What it does:** Two pure AST-to-AST rewriters: `rewrite_idents_with_prefix` (prefixes identifiers whose leading segment is in a field set, used to inherit type-body constraints onto named instances) and `substitute_bound_var` (replaces a bound variable name and its dotted suffixes with an element expression, used for `∀` unrolling). Both are total recursive traversals of the Evident `Expr` AST.
**Criticality:** critical
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** These are AST transformation passes that produce the rewritten constraint ASTs fed to the Z3 translator. They are part of the compile front-end (the expand-before-translate pipeline); the output of these rewrites is what Z3 receives. Replacing them with a Z3 solve would be circular.
**Change made:** none
