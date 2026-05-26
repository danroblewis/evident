# runtime/src/parser/patterns.rs — Z3-replaceability
**What it does:** Parses `match` expressions (indent-delimited arms, each `Pattern ⇒ body`) and `MatchPattern` values (`_` wildcard, lowercase bind names, uppercase nullary constructors, `Ctor(p…)` payload patterns). Used both by the `match` expression parser and the `e matches Pattern` recognizer form.
**Criticality:** critical
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** Pattern parsing is a recursive token-stream → AST transform. It classifies tokens by capitalization and punctuation into `MatchPattern::Wildcard`, `MatchPattern::Bind`, or `MatchPattern::Ctor` variants. There is no search or constraint satisfaction involved — it is deterministic grammar traversal. Z3 needs these `MatchPattern` nodes to already exist; the parser is the thing that creates them. Circular by construction.
**Change made:** none
