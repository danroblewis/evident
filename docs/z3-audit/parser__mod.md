# runtime/src/parser/mod.rs — Z3-replaceability
**What it does:** Root of the parser module: declares the `Parser` struct with its token-cursor state (`toks: Vec<Token>`, `pos: usize`), provides the shared `peek`/`bump`/`eat`/`skip_blank_newlines` primitives, a `peek_compare_op` helper used by chained-comparison detection, and the public `parse(src: &str) -> Result<Program>` entry point that chains lexer → parser.
**Criticality:** critical
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** This is the entry point of the text→AST pipeline (`tokenize` → `parse_program`). The `Parser` struct manages cursor state over a flat token vector; its helpers are token-stream navigation primitives, not constraint relations. A Z3 solve operates on AST nodes — it cannot substitute for the machinery that produces ASTs from source text. Circular by construction.
**Change made:** none
