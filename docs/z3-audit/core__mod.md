# runtime/src/core/mod.rs — Z3-replaceability

**What it does:** Module root for `core/` — declares and re-exports `ast`, `value`, `z3_types`, `z3_program`, `api`, `functionizer`, and `seq_helpers` as a flat public namespace. No logic of its own.

**Criticality:** critical

**Verdict:** not-a-CSP

**Confidence:** high

**How (if replaceable):** Not applicable. This is a 17-line Rust module file consisting entirely of `pub mod` declarations and `pub use` re-exports. It is structural plumbing for the Rust module system, not an algorithm. Nothing to solve.

**Change made:** none
