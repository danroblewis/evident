# runtime/src/commands/sample.rs — Z3-replaceability
**What it does:** Implements `evident sample`: calls `rt.sample()` (blocking-clause loop for distinct models) and formats results as human-readable or JSON. `--all` mode iterates every schema and reports SAT/UNSAT for each. Includes helpers to skip generic templates and bare-Seq library claims.
**Criticality:** peripheral
**Verdict:** not-a-CSP
**Confidence:** high
**How (if replaceable):** CLI orchestration and output formatting; the constraint solving is entirely inside `rt.sample()` / `rt.query()`. The skip predicates (`has_generic_seq_param`, `is_generic_template`) are trivial AST field checks — no search problem. JSON serialization is pure string transformation. Spec in one line: `sample(schema, n) = {m₁, …, mₙ | m ⊨ schema, mᵢ ≠ mⱼ}` (already implemented by rt.sample).
**Change made:** none
