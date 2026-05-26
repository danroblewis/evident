# runtime/src/parser/atoms.rs — Z3-replaceability
**What it does:** Parses atom-level expressions from a token stream: integer/real/bool/string literals, `match` expressions, dotted identifier chains, generic constructor calls like `Edge<Rect>(args)`, `run(F, init)` nested-FSM form, parenthesized/tuple expressions, set/range literals `{…}`, and sequence literals `⟨…⟩`. Produces `Expr` AST nodes.
**Criticality:** critical
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** This is the recursive-descent bottom of the expression grammar. It consumes a `Vec<Token>` produced by the lexer and builds `Expr` AST nodes. A Z3 solve presupposes an AST already exists — you need the parser to produce the AST before Z3 can reason about anything. Replacing this with a Z3 solve is logically circular: the parser is the thing that creates the structure a solve would operate on.
**Change made:** none
