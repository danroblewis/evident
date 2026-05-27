# runtime/src/translate/exprs/record_lift.rs — Z3-replaceability
**What it does:** Broadcasts comparison and equality operators (`=`, `≠`, `<`, `≤`, `>`, `≥`) componentwise over the leaf fields of a record-typed expression. Given `a ≤ b` where both sides are `IVec2`, it rewrites to `a.x ≤ b.x ∧ a.y ≤ b.y` and translates each per-field sub-expression to a Z3 Bool. Also walks record literals, dotted field chains, and `seq[i].field` shapes.
**Criticality:** critical
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** This is a structural rewriting pass that happens inside the Evident→Z3 translation loop; its output is a Z3 `Bool<'ctx>` AST node that the solver then consumes. It is not computing an answer to a decidable property — it is synthesising the Z3 expression that encodes the constraint. Self-hosting it would require Z3 to already be running to produce the expression Z3 needs to run on.
**Change made:** none
