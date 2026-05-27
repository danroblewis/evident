# runtime/src/translate/exprs/range.rs — Z3-replaceability
**What it does:** Resolves a `Range(lo, hi)` Evident AST node into a concrete `(i64, i64)` pair by calling `translate_int` on each bound and then asking Z3 to simplify and extract an integer literal. This feeds the `∀ i ∈ {0..n-1}` unroller, which requires pinned numeric bounds to generate a finite set of per-index constraints.
**Criticality:** critical
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** This function IS part of the Evident→Z3 translation pipeline. It calls `translate_int` (itself a translator) and queries Z3's simplifier to extract a bound, then returns that bound to the loop that generates Z3 constraints. Replacing it with a Z3 solve would require the compiler to already be running — it is literally the step that prepares the range so the compiler can proceed.
**Change made:** none
