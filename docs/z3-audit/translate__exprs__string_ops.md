# runtime/src/translate/exprs/string_ops.rs — Z3-replaceability
**What it does:** Lowers Evident string built-ins to Z3 sequence theory via raw `z3-sys` calls (`Z3_mk_seq_length`, `Z3_mk_seq_extract`, `Z3_mk_seq_replace`, `Z3_mk_seq_index`, `Z3_mk_seq_at`, `Z3_mk_int_to_str`). Exposes `translate_str_call` / `translate_str_int_call` / `translate_str_bool` dispatch tables consumed by `scalar.rs` and `bool.rs`. This is the "substring primitive" unlock identified in multiple prior self-hosting sessions (GAPC).
**Criticality:** critical
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** This file builds the Z3 string-theory AST nodes that are subsequently passed to the Z3 solver. It is the bridge between Evident string syntax and Z3's sequence theory — it IS the encoding step, not a step that could be encoded. The prior sessions correctly noted it as the unlock for *other* self-hosted passes, but the file itself remains firmly in the compiler pipeline: you cannot use Z3 str.* to produce the Z3 str.* term before the term exists.
**Change made:** none
