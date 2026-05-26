# runtime/src/translate/exprs/scalar.rs — Z3-replaceability
**What it does:** Per-sort translators (`translate_str`, `translate_int`, `translate_real`, `real_from_f64`) that walk an Evident `Expr` and produce the corresponding Z3 AST term (Int, Real, or String). Handles literals, identifiers, arithmetic, cardinality (`#seq`), sequence indexing, field access, ternary ITE, match-to-ITE, and built-in functions (`min`, `max`, `abs`, `mod`, `clamp`, `position_of`).
**Criticality:** critical
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** These functions ARE the expression front-end of the compiler — they convert source-language expressions into Z3 AST nodes that the solver will evaluate. There is no algorithm being decided here; the output is Z3 input. Replacing them with a Z3 solve is logically incoherent: Z3 cannot produce the terms it needs to reason about before those terms exist.
**Change made:** none
