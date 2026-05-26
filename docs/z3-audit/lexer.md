# runtime/src/lexer.rs — Z3-replaceability
**What it does:** Tokenizes Evident source text into a stream of `Token` variants; handles Unicode operators (∈, ∧, ⇒, etc.) and indentation-significance (Newline/Indent tokens).
**Criticality:** critical
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** Not a CSP. Tokenization is pure string → token-stream transformation; it IS the front end of the pipeline that feeds everything else including the solver. Replacing it with Z3 would require a Z3 context before any source is parsed — classic circularity.
**Change made:** none
