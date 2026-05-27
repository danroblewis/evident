# runtime/src/translate/eval/decode.rs — Z3-replaceability
**What it does:** Decodes Z3 model values back into Rust `Value`s. `extract_binding` dispatches per `Var` kind (Int, Bool, Real, Str, Seq, Set, Enum); `extract_enum_value` recursively walks Z3 datatype testers and accessors to reconstruct `Value::Enum` with nested payloads including Seq fields and internal-Cons chains.
**Criticality:** critical (called by every evaluate* path after every successful solve to produce the `QueryResult::bindings` map)
**Verdict:** not-a-CSP
**Confidence:** high
**How (if replaceable):** Model extraction is not a constraint problem — it is the output-side of the solve. After Z3 returns SAT, this code interrogates the Z3 model object via the C API (testers, accessors, `model.eval`) to reconstruct typed Rust values. There is no decision problem here to express as a constraint; it is pure data extraction from an already-computed model.
**Change made:** none
