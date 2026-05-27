# runtime/src/translate/exprs/seq_eq.rs — Z3-replaceability
**What it does:** Translates sequence and set equality constraints into Z3 — `seq = ⟨a, b, c⟩` (Cons/Nil enum chains, primitive Seq, composite Seq), `S = {…}` (set literals with candidate recording), whole-Seq equality (element-wise conjunction), and `seq[i] = record` assignment. Also provides `bind_composite_fields` (used by quantifier/mapping paths to project Datatype accessor results back into env) and `match_set_subset_body` (detects the `∀ x ∈ A : x ∈ B` subset pattern to emit native Z3 `set_subset`).
**Criticality:** critical
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** Every function in this file produces a Z3 `Bool<'ctx>` or mutates the translation environment — it is building Z3 AST nodes, not deciding a property. The "equality" here is the constraint being encoded into Z3, not something Z3 is being asked to check. Replacing this with a Z3 solve would require the encoding to already exist.
**Change made:** none
