# runtime/src/parser/program.rs — Z3-replaceability
**What it does:** Parses the top-level `Program` structure: dispatches on `schema`/`claim`/`type`/`fsm`/`external`, `import`, and `enum` keywords; implements `parse_enum_decl` (single-line `|`-separated, indented multi-line with/without leading `|`, payload variants with recursive field types) and `parse_enum_field_type` (recursive compound type names like `Seq(Expr)`).
**Criticality:** critical
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** Top-level program parsing builds the `Program` struct (schema list + enum list + import list) from a token stream. The enum parser handles multi-line layout disambiguation, optional leading pipes, recursive payload types, and variant-continuation detection — all token-position logic. Z3 constraints operate on a `Program` AST; this file is what constructs that AST from text. Circular by construction.
**Change made:** none
