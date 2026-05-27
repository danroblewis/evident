# runtime/src/translate/datatypes.rs — Z3-replaceability
**What it does:** Builds and caches `Z3 DatatypeSort` objects for user-defined types used as `Seq(UserType)` elements. Recursively constructs sorts for nested fields (primitives, Seqs, enums, nested user types) and caches them in a `DatatypeRegistry` so sibling schemas sharing the same element type reuse one Z3 sort.
**Criticality:** critical (load-time, on the translate pipeline)
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** This file IS the compile pipeline: it directly constructs `z3::DatatypeSort` objects by calling Z3's `DatatypeBuilder` API. These sorts are the Z3-level type representations that the constraint translator then uses to build Z3 ASTs. You cannot replace "the thing that creates Z3 sorts" with "a Z3 solve" — the sorts must exist before any solve can be set up.
**Change made:** none
