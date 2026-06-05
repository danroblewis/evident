# Evident notes — invariants and gotchas worth remembering

Things that are non-obvious about the Evident language and runtime,
condensed from the parent project's `CLAUDE.md`. Skim this when working
on the Rust port so you don't reinvent decisions or miss footguns.

## Three keywords, same AST node

`schema`, `claim`, and `type` all parse to the same node (`SchemaDecl`
in Python). They differ only by reading convention. The Rust AST should
keep a `keyword: Keyword` enum on `SchemaDecl` so the distinction
isn't lost — a few features (e.g., `subclaim` vs claim) check it.

## Boolean literals are `true` / `false` (lowercase)

`True` / `False` parse as unbound identifiers — the constraint silently
becomes wrong. Lex `true` / `false` as Bool literals.

## `⇒` binds tighter than `∧`

Opposite of standard math convention. `A ⇒ B ∧ C` parses as
`(A ⇒ B) ∧ C`. Don't "fix" this — match the existing grammar.

## Unicode operator mapping

Python's `parser/src/normalizer.py` rewrites these to ASCII keywords
before Lark sees them:

| Glyph | Replacement   | Meaning                        |
|-------|---------------|--------------------------------|
| `∈`   | `__IN__`      | membership                     |
| `∉`   | `__NOTIN__`   | non-membership                 |
| `∧`   | `__AND__`     | logical and                    |
| `∨`   | `__OR__`      | logical or                     |
| `¬`   | `__NOT__`     | logical not                    |
| `⇒`   | `__IMPLIES__` | implies                        |
| `⟸`   | `__REVIMPL__` | reverse implies (sugar)        |
| `≤`   | `<=`          | less or equal                  |
| `≥`   | `>=`          | greater or equal               |
| `≠`   | `__NEQ__`     | not equal                      |
| `∀`   | `__FORALL__`  | for all                        |
| `∃`   | `__EXISTS__`  | exists                         |
| `↦`   | `__MAPSTO__`  | rename in claim composition    |
| `∪`   | `__UNION__`   | set union                      |
| `∩`   | `__INTERSECT__` | set intersection             |
| `⊆`   | `__SUBSET__`  | subset                         |
| `⟨…⟩` | `[…]`         | sequence literal               |
| `{…}` | `{…}`         | set literal                    |
| `#x`  | `__CARD__ x`  | cardinality                    |

Word forms: `in`, `not in`, `subset`, `superset`, `mapsto` rewrite to
the same `__TOKEN__` form.

For Rust we can either lex the Unicode directly (preferred) or do a
normalization pre-pass. Direct lexing is fewer moving parts.

## Indentation is significant

The Python grammar uses Lark's INDENT/DEDENT tokens. Schema bodies are
introduced by indenting under the schema head. For a hand-rolled Rust
lexer we'll need to track indentation levels — every newline-then-more-
spaces opens a block; every newline-then-less-spaces closes one.

Comments start with `--` (run to end of line). They're stripped before
the indent rules are applied.

## Sub-schema field expansion

`task ∈ Task` doesn't create a Z3 constant named `task`. It creates
`task.id`, `task.duration`, etc. — one per field of `Task`. Field
access in expressions resolves through dotted env lookups, not via
Z3 datatype accessors (for non-composite cases). The Python runtime
intercepts `BinaryExpr(×, Identifier('task'), FieldAccess('.', 'duration'))`
in `translate.py` and turns it into `env.lookup('task.duration')`.

For v0.1 we can skip sub-schema expansion entirely — only support
flat `n ∈ Nat` declarations.

## Naturals are `Int` with `>= 0` constraint

`Nat` isn't a separate Z3 sort. It's `Int` with a side-constraint
`x >= 0` added when the variable is declared. Same for `Pos` (`> 0`).

## Z3 AST identity matters

In Python, `runtime/src/ast_types.py` re-exports parser AST classes via
the package importer to ensure isinstance checks work across loads. The
Rust port doesn't have this problem (single crate, single type for each
AST node).

## Cached evaluator is an optimization, not the API

The Python runtime gained `evaluate_cached()` recently — it caches
translated constraints and reuses a Z3 solver via push/pop. Don't worry
about this in the Rust port until the basic uncached path works.
