# runtime/src/lib.rs — Z3-replaceability
**What it does:** Library crate root: declares all internal modules, re-exports the public API surface (`EvidentRuntime`, `Value`, `QueryResult`, `RuntimeError`, `ast`), and exposes `parse_program` as a testing convenience that runs the parser without the full load pipeline.
**Criticality:** critical
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** This file IS the crate boundary — it wires together the parser, translator, functionizer, effect loop, and every other piece of the solve machinery. Replacing it with a Z3 solve would require Z3 to orchestrate the very infrastructure that runs Z3. Spec in one line: `lib = {parse, translate, functionize, effect_loop, ffi, …}` (the runtime itself).
**Change made:** none
