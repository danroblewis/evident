# runtime/src/runtime/register_enums.rs — Z3-replaceability
**What it does:** Registers `enum` declarations as Z3 Datatype sorts via `z3::datatype_builder::create_datatypes`. Handles forward/mutual references by batch-staging (union-find + topological sort of dependency groups), generates internal Cons-list helpers for `Seq(T)` fields where T is batch-local, and validates variant-name global uniqueness.
**Criticality:** critical (load-time; must run before any schema that references an enum can be translated)
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** This file produces Z3 sort declarations — it IS the step that creates Z3 types. Z3 cannot reason about `enum Effect` until `register_enums` has called `create_datatypes` and stored the resulting `DatatypeSort` in the registry. The topological staging logic (union-find + Kahn-style group ordering) is a graph algorithm that could be expressed as a constraint in principle, but it must complete before Z3 can accept any assertion that mentions an enum sort — circular. The uniqueness checks are load-time validation, not CSPs.
**Change made:** none
