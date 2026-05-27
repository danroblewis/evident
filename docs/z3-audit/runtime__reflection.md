# runtime/src/runtime/reflection.rs — Z3-replaceability
**What it does:** The self-hosting reflection bridge: encodes the loaded user program AST as a Z3 Datatype value (matching `stdlib/ast.ev`'s `Program` enum), then provides `query_with_program`, `query_with_nth_claim_body_only`, and related variants that inject the encoded AST into self-hosted pass queries. Used by all mode-1 self-hosted compiler passes (validate, subscriptions, generics, desugar, etc.).
**Criticality:** critical (load-time; gating path for all self-hosted passes)
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** This file is the bridge that feeds the AST into Z3 so Evident passes can reason over it. Replacing it with a Z3 solve would require this bridge to already be running in order to supply the program value to Z3. The encode step (`encode_program_value`) converts Rust AST structs to Z3 Datatype AST nodes — this is Z3 input construction, not a CSP. The `user_program` / `mark_system_loads_complete` boundary tracking is bookkeeping for the self-host pipeline, not a solvable property.
**Change made:** none
