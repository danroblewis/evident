# runtime/src/parser/types.rs — Z3-replaceability
**What it does:** Parses type-name and pin-clause forms: `try_parse_generic_args_suffix` consumes `<arg, …>` recursively returning the suffix as a string; `try_parse_type_and_pins` handles bare types, compound heads (`Seq(Int)`, `Set(Edge<T>)`), generic instantiations (`Edge<Rect>`), named pin clauses (`Type(slot ↦ value, …)`), and positional pin clauses (`Type(v1, v2)`). Returns `(type_name_string, Pins)` pairs.
**Criticality:** critical
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** Type-name parsing is lookahead-driven token disambiguation: it inspects 1–3 tokens ahead to decide among several lexically similar forms (compound type vs named-pin vs positional-pin vs generic instantiation). The result feeds `BodyItem::Membership.type_name` and `Pins`. All logic is token-stream navigation; no search or constraint satisfaction is involved. Z3 needs these `Pins` values to already exist in the AST. Circular by construction.
**Change made:** none
