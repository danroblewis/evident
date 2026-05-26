# runtime/src/core/z3_program.rs — Z3-replaceability

**What it does:** Defines `Z3Program` (the IR between the translator and the functionizer/solver: topo-ordered steps, consistency checks, residual predicates), `Z3Step` (Scalar/Seq/Guarded/PreBaked/SampleRange/SampleEnum/SampleSet variants), `GuardedBranch`, and `GuardedBody`. Also provides `Display` impls and a smoke test for all step shapes.

**Criticality:** critical

**Verdict:** not-a-CSP

**Confidence:** high

**How (if replaceable):** Not applicable. This is the IR data structure that sits between translation (AST → Z3 ASTs) and compilation (Z3Program → Cranelift JIT / Z3 solve). It holds live `z3::ast::*` handles (`Bool<'ctx>`, `Dynamic<'ctx>`) with Z3 context lifetimes baked in. It IS the input to the solver — you cannot replace the type definition for what you feed to Z3 with Z3 itself. Circular by construction: this file is part of the solve machinery.

**Change made:** none
