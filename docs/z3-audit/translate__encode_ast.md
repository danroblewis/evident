# runtime/src/translate/encode_ast.rs — Z3-replaceability
**What it does:** Encodes a Rust `ast::Program` as Z3 `Datatype` values matching `stdlib/ast.ev`'s enum definitions. Provides two parallel encoders: a Z3-AST encoder (for pinning AST values as Z3 assertions) and a `Value::Enum` encoder (`*_to_value` functions) for the stack-FSM self-hosted passes that operate on extracted model values without touching Z3 directly.
**Criticality:** critical (load-time, on the self-hosting bridge)
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** This file IS the bridge that feeds Evident AST into Z3 as data, enabling self-hosting. The encode direction (Rust AST → Z3 datatype constructor calls) is necessarily prior to any Z3 solve — you need the encoded representation in order to pin it as an assertion before solving. Replacing the encoder with a solve is circular: the encoder is what produces the input the solve would consume.
**Change made:** none
