# runtime/src/translate.rs — Z3-replaceability
**What it does:** Module facade for the `translate/` subtree: re-exports public API from `eval`, `preprocess`, `extract`, and `encode_ast`/`decode_ast` decoders. Declares submodules and the `smtlib` prototype. Contains no logic itself (~60 lines of `mod` + `pub use`).
**Criticality:** critical
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** This file IS the AST→Z3 translation driver (the pipeline stage that builds the solver). Replacing it with a Z3 solve would be circular — you need the translator to run any solve in the first place. The file itself is just re-exports; the actual translation logic lives in the submodules it declares.
**Change made:** none
