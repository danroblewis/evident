# runtime/src/z3_eval.rs — Z3-replaceability
**What it does:** Extracts a `Z3Program` IR from simplified Z3 ASTs — walks the simplified formula set, partitions assertions into per-output assignments vs. consistency checks, and builds the ordered `Z3Step` list the Cranelift JIT compiles. Re-exports `Z3Program`, `Z3Step`, `GuardedBranch`, `GuardedBody` from `core`.
**Criticality:** critical
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** This IS the IR extractor that bridges Z3 AST → the JIT-compilation IR. It operates on already-built Z3 AST nodes and uses Z3 tactic machinery (`simplify`, `propagate-values`) as preprocessing. Replacing it with a Z3 solve would require Z3 to reason about its own AST structure — deeply circular. The output (`Z3Program`) is the input to the functionizer, not a model over user variables.
**Change made:** none
