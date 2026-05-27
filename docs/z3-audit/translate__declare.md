# runtime/src/translate/declare.rs — Z3-replaceability
**What it does:** Declares Z3 constants for typed variables by inserting `z3::ast::Int/Bool/Real/String/Array/Set/Datatype` constants into the translation environment (`env`). Also provides helpers to pin Seq lengths to literals (`apply_seq_lengths`) and populate Set candidates (`apply_set_candidates`) before body translation.
**Criticality:** critical (load-time, core of the translate pipeline)
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** This file IS the compile pipeline: it creates `z3::ast::*` constants (the Z3 variables that will appear in the constraints being built). Replacing "the thing that allocates Z3 constants" with "a Z3 solve" is circular — the Z3 constants must exist before Z3 can be asked to solve anything. The `apply_seq_lengths` / `apply_set_candidates` helpers are also pre-solve setup steps, not decisions that could be outsourced to a solve.
**Change made:** none
