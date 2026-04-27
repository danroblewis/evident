# Evident Specification — Lexical Syntax

## Keywords

Reserved words. Cannot be used as identifiers.

```
schema   claim    type     assert   evident
∀        ∃        ∃!       ¬∃
∈        ∉        ⊆        ⊇
∩        ∪        ×        \
∧        ∨        ¬
⇒        ↦
·        ⋈
```

## ASCII aliases

Every Unicode symbol has an ASCII alias. The editor replaces ASCII input with
Unicode on display. Both forms are accepted by the parser; they are identical.

| ASCII input | Unicode | Meaning |
|---|---|---|
| `in` | `∈` | set membership |
| `not in` | `∉` | non-membership |
| `subset` | `⊆` | subset |
| `superset` | `⊇` | superset |
| `intersect` | `∩` | intersection |
| `union` | `∪` | union |
| `cross` | `×` | Cartesian product |
| `\` | `\` | set difference (same in both) |
| `and` | `∧` | logical AND |
| `or` | `∨` | logical OR |
| `not` | `¬` | logical NOT |
| `all` | `∀` | universal quantifier |
| `some` | `∃` | existential quantifier |
| `one` | `∃!` | unique existential |
| `none` | `¬∃` | no element satisfies |
| `=>` | `⇒` | forward implication |
| `mapsto` | `↦` | variable mapping (in composition blocks) |
| `<=` | `≤` | less than or equal |
| `>=` | `≥` | greater than or equal |
| `!=` | `≠` | not equal |

## Editor shortcuts (typed with backslash)

| Shortcut | Unicode |
|---|---|
| `\in` | `∈` |
| `\notin` | `∉` |
| `\->` or `\to` | `→` |
| `\=>` or `\Rightarrow` | `⇒` |
| `\exists` or `\ex` | `∃` |
| `\forall` or `\all` | `∀` |
| `\leq` | `≤` |
| `\geq` | `≥` |
| `\neq` | `≠` |
| `\subset` | `⊆` |
| `\mapsto` | `↦` |
| `\cdot` | `·` |
| `\bowtie` | `⋈` |

## Identifiers

```
identifier  ::= [a-z_][a-zA-Z0-9_]*
type_name   ::= [A-Z][a-zA-Z0-9_]*
internal    ::= '_' identifier        -- body-internal scaffolding variable
```

Lowercase identifiers are variables and claim names.
Uppercase identifiers are type names and type constructors.
Identifiers beginning with `_` are body-internal (implicitly existential, no domain meaning).

## Literals

```
nat_literal    ::= [0-9]+
int_literal    ::= '-'? [0-9]+
real_literal   ::= [0-9]+ '.' [0-9]+
string_literal ::= '"' [^"]* '"'
bool_literal   ::= 'true' | 'false'
```

## Comments

```
-- single line comment to end of line
```

No block comments.

## Whitespace and layout

Evident uses significant indentation. The body of a claim is the block of
lines indented below the claim head. Indentation uses spaces (not tabs).

```evident
claim my_claim
    variable ∈ Type      -- indented: part of the body
    constraint           -- indented: part of the body

claim next_claim         -- not indented: new top-level declaration
```

## Operator precedence

From highest to lowest binding:

1. `.` field access and projection
2. `[condition]` filter
3. `|S|` cardinality
4. Arithmetic: `*`, `/`
5. Arithmetic: `+`, `-`
6. Comparison: `=`, `≠`, `<`, `>`, `≤`, `≥`, `∈`, `∉`, `⊆`
7. `¬`
8. `∧`
9. `∨`
10. `⇒`
11. `∀`, `∃`
12. `·`, `⋈` (composition)

## Open questions

- Composition operator: `·` (middle dot) vs `⋈` (bowtie) — not yet committed
- Tab vs space indentation
- Maximum line length recommendation
