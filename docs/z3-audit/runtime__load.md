# runtime/src/runtime/load.rs — Z3-replaceability
**What it does:** Source loading pipeline: parses Evident source text, resolves and recursively loads imports (cycle-detection via canonicalized path set), runs all pre-translation passes in order (unify_world, unify_state, desugar_seq_concat, inject passes, validate), registers enums with Z3 (`create_datatypes`), expands generics, rewrites embedded FSM applications, and flushes all caches. Also provides import path resolution (verbatim → relative → cwd → ancestor walk).
**Criticality:** critical (load-time; this is the entry point for all schema loading — nothing can be queried without it)
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** This file IS the bootstrap loader — it calls the parser, drives the desugar/inject passes, calls `register_enums` to declare Z3 datatypes, and invokes `monomorphize_generics`. Replacing it with a Z3 solve is circular: you need to run this pipeline to load ANY Evident source, including any Evident source you'd write to replace it. The import resolver is pure filesystem IO. The orchestration logic has no decidable property; it is a fixed-order pipeline over mutable runtime state.
**Change made:** none
