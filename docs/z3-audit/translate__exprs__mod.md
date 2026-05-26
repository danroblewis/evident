# runtime/src/translate/exprs/mod.rs — Z3-replaceability
**What it does:** Module root for `translate/exprs/`: declares and re-exports the sub-modules (`bool`, `enums`, `mapping`, `scalar`, `record_lift`, `seq_eq`, `quant`, `match_expr`, `range`, `string_ops`) and defines two thread-local translation contexts — `ACTIVE_ENUMS` (the active `EnumRegistry` pointer) and `TARGET_ENUM_HINT` (the SeqLit-as-Cons-chain target enum hint) — plus their RAII guard/accessor helpers.
**Criticality:** critical
**Verdict:** trivial
**Confidence:** high
**How (if replaceable):** This is module glue: re-exports of sibling modules plus two thread-local context cells with trivial RAII wrappers. There is no algorithm or logic here to replace with a Z3 solve; it is pure module plumbing that wires the translation pipeline together.
**Change made:** none
