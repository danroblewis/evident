# runtime/src/parser/schema.rs — Z3-replaceability
**What it does:** Parses schema/claim/type/fsm declarations: `external` modifier, generic type parameter lists (`<T, U>`), first-line param shorthand (`type Vec2(x, y ∈ Int)`), and indented bodies via `parse_indented_body`. Also handles `subclaim Name` body items by parsing them as nested `SchemaDecl` nodes. Tracks `param_count` to distinguish interface params from body items.
**Criticality:** critical
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** Schema declaration parsing builds `SchemaDecl` AST nodes (keyword, name, type_params, body, param_count, external flag) from a token stream. The indent-tracking body loop, generic-arg parsing, and param-count bookkeeping are all token-position control flow. Z3 constraints presuppose `SchemaDecl` nodes exist — this file is what produces them. Circular by construction.
**Change made:** none
