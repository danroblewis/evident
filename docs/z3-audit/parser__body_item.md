# runtime/src/parser/body_item.rs — Z3-replaceability
**What it does:** Parses body items inside a schema/claim/fsm body: passthrough (`..TypeName`), subclaim declarations, `halts_within(F, N)` forms, named-pin claim calls (`Name(slot ↦ value, …)`), regular membership declarations (`name ∈ TypeName`), chained-membership shorthand (`0 < x ∈ Int < 5`), multi-name declarations, and fallthrough constraint expressions. Produces `Vec<BodyItem>` AST nodes.
**Criticality:** critical
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** Body-item parsing converts a flat token stream into structured AST nodes (`BodyItem::Membership`, `BodyItem::Constraint`, `BodyItem::ClaimCall`, etc.). This is a text→AST transform. A Z3 solve requires an AST as input — it cannot construct the AST from text. The parser is the AST-producing machinery a solve presupposes, making replacement circular by construction.
**Change made:** none
