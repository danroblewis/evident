# runtime/src/commands/test.rs — Z3-replaceability
**What it does:** Implements `evident test [path]`: discovers `test_*.ev` files, queries every `sat_*`/`unsat_*` claim, reports pass/fail with ANSI-colored output. On `sat_*` failure shows the UNSAT core; on `unsat_*` failure shows the SAT counterexample with per-constraint binding annotation. Includes an ANSI syntax highlighter for constraint text and a binding flattener for composite values.
**Criticality:** peripheral
**Verdict:** not-a-CSP
**Confidence:** high
**How (if replaceable):** Test discovery is filesystem traversal; test execution delegates entirely to `rt.query` / `rt.query_with_core`. Output formatting (ANSI highlighting, binding flattening, counterexample display) is pure string/data transformation. The `referenced_names_in` helper is a shallow AST walk with no search. Spec in one line: `test(files) = ∀ sat_* ∈ files: query(claim).sat ∧ ∀ unsat_* ∈ files: ¬query(claim).sat`.
**Change made:** none
