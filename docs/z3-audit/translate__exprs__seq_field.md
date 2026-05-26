# runtime/src/translate/exprs/seq_field.rs — Z3-replaceability
**What it does:** Provides two resolvers used by scalar and seq_eq translation: `resolve_seq_handle` turns a Seq-typed `Expr` (bare identifier or nested `groups[0].items` Field-of-Index shape) into a uniform `SeqHandleRef` carrying the Z3 array, length, and element type; `resolve_seq_field` walks a `seq[idx].field.subfield` chain and returns the Z3 Dynamic leaf plus its type name.
**Criticality:** critical
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** This is pure environment-and-AST traversal that extracts Z3 AST handles from the translation environment. Its output is Z3 array/accessor expressions used directly by index, cardinality, and quantifier translation. It is infrastructure for producing Z3 input, not a standalone decision procedure.
**Change made:** none
