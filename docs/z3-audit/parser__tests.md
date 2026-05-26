# runtime/src/parser/tests.rs — Z3-replaceability
**What it does:** Unit test suite for the parser: 20+ `#[test]` functions covering membership parsing, cardinality/index expressions, arithmetic precedence, chained-membership desugaring (two-sided, pin form, compound types, multi-name, set-membership non-interference), enum declarations (basic, payload variants, recursive/mutual, multiline with/without leading pipe, error cases), and rejection of dotted LHS in chained membership.
**Criticality:** peripheral
**Verdict:** not-a-CSP
**Confidence:** high
**How (if replaceable):** Test code — not production logic, not a constraint problem. Exercises the parser via round-trip assertions on AST shape. No Z3 involvement possible or relevant; tests are executable specifications of the parser's grammar rules.
**Change made:** none
