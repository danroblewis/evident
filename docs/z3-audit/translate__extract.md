# runtime/src/translate/extract.rs — Z3-replaceability
**What it does:** Extracts model values from a satisfied Z3 solver back into Rust `Value` types (`extract_seq`, `extract_seq_composite`, `extract_composite_value`, `extract_set`), and pins Seq/Set variables to given `Value` maps by building Z3 equality assertions (`assert_seq_given`, `assert_set_given`). Also provides the Unicode-safe Z3 string encode/decode helpers (`z3_string`, `unescape_z3_string`).
**Criticality:** critical (load-time and tick-level result extraction)
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** This file is pure IO/marshaling between Z3 model outputs and Rust values — the read-back direction of the solve pipeline. Extracting "what values Z3 found" necessarily happens after solving and requires direct inspection of the Z3 model API. The pinning functions (`assert_seq_given`) construct Z3 Boolean assertions to add to the solver, which is compile-pipeline work. Neither direction is a decision problem that could be replaced by a separate solve.
**Change made:** none
