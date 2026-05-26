# runtime/src/decompose.rs — Z3-replaceability
**What it does:** Union-find decomposition of a claim's Z3 assertions into disjoint connected components over free variables; purely structural, no `check()` calls. Used in `translate/eval/decompose.rs` and `runtime/query.rs` to split large models before solving.
**Criticality:** peripheral
**Verdict:** not-a-CSP
**Confidence:** high
**How (if replaceable):** The algorithm is a graph/union-find traversal over Z3 AST nodes — it partitions constraints by variable co-occurrence. This is a structural preprocessing step that runs *before* any Z3 solve; it has no satisfiability question to pose. A Z3 solve couldn't replace it: the output is a partitioning of constraint indices, not a model. The work is linear in formula size and strictly faster than any SAT call.
**Change made:** none
