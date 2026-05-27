# runtime/src/runtime/mod.rs — Z3-replaceability
**What it does:** Declares the `EvidentRuntime` struct (all runtime state: schemas map, Z3 context, solver caches, JIT function cache, value cache, autotune history, enum registry, datatype registry, schema origins, loaded-file set, system boundary marker) and wires the submodule tree. Provides `new()`, `with_functionizer()`, and thin accessor methods (`schema_names`, `get_schema`, `enums_registry`, `z3_context`, etc.).
**Criticality:** critical (load-time and tick-0; this is the top-level API struct — every path through the runtime touches it)
**Verdict:** not-a-CSP
**Confidence:** high
**How (if replaceable):** This file is the data-structure definition and module registry for the entire runtime. It holds the Z3 context (`Box::leak`'d `'static` reference), all caches, and the pluggable functionizer. There is no algorithm here — only struct declaration, constructor, and accessor delegation. Not a constraint-satisfaction problem; it is the container that makes Z3 solving possible.
**Change made:** none
