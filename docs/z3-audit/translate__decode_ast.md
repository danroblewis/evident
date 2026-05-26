# runtime/src/translate/decode_ast.rs — Z3-replaceability
**What it does:** Decodes a `Value::Enum` tree (produced by extracting a Z3 model value) back into a Rust `ast::Program`. This is the read-side of the self-hosting reflection bridge: after a self-hosted Evident pass transforms the AST (encoded as Z3 datatype values), this file converts the Z3 model's output back into Rust AST nodes so the runtime can continue processing.
**Criticality:** critical (load-time, on the self-hosted pass pipeline)
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** This file is the bridge that makes self-hosting possible — it is the decoder that extracts the result of a Z3 solve (the transformed AST) back into Rust data. Without it, the self-hosted passes' outputs cannot be consumed. Replacing the decoder with a Z3 solve would be circular: the decoder is what converts Z3 model values into the Rust representation that the next step of the compile pipeline consumes. It is IO/marshaling infrastructure, not a decision problem.
**Change made:** none
