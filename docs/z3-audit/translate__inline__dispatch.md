# runtime/src/translate/inline/dispatch.rs — Z3-replaceability
**What it does:** Resolves dotted call names (e.g. `recv.subclaim(args)`) to one of three `CallDispatch` flavors: `Subschema` (receiver has the subclaim on its type), `ReceiverPrefix` (suffix is a known claim), or `Plain` (whole name is a known claim). Also provides `resolve_forall_unroll` which statically unrolls `coindexed`/bare-identifier `∀` ranges into per-index bindings.
**Criticality:** critical
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** This file is part of the front-end name-resolution pass that decides how to expand claim calls before Z3 ever sees the constraints. The dispatch logic depends on walking the schema table and body items — it is compile-time analysis that produces the input to Z3, not a problem Z3 can solve. Replacing it with a Z3 solve would be circular.
**Change made:** none
