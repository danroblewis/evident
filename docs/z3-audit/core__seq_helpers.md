# runtime/src/core/seq_helpers.rs — Z3-replaceability

**What it does:** Two pure string utility functions: `parse_seq_type("Seq(T)") → Some("T")` (strips the wrapper) and `internal_cons_helper_name("T") → "__SeqOf_T"` (constructs the internal Cons-enum name for Seq types). Called from translate/, runtime/, and effect_loop/.

**Criticality:** peripheral

**Verdict:** trivial

**Confidence:** high

**How (if replaceable):** These are two 3-line string operations — a prefix/suffix strip and a format string. The spec as a constraint would be: `∃ t: s = "Seq(" ++ t ++ ")"`. That constraint is strictly slower to solve than the 3-line Rust (Z3 string theory overhead, context setup, etc.), produces no information the Rust doesn't already have, and the function is called from hot paths (translate/ during every schema load). A solve buys nothing here.

**Change made:** none
