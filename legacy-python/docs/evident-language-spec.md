# Evident Language Reference Specification

A faithful extraction of the language as implemented on the `main` branch's Rust
runtime. This is the source of truth: the lexer (`runtime/src/lexer.rs`),
parser (`runtime/src/parser/`), AST (`runtime/src/core/ast.rs`), and translator
(`runtime/src/translate/`).

This doc is annotated with **DISCREPANCY** notes wherever the bootstrap
implementation on the `tiny-runtime` branch (Python under `src/`) differs from
the Rust reference. Those notes flag corrections the bootstrap needs.

---

## Table of Contents

1. [Lexical structure](#1-lexical-structure)
2. [Top-level declarations](#2-top-level-declarations)
3. [Schemas: type / claim / schema / fsm](#3-schemas-type--claim--schema--fsm)
4. [Body items](#4-body-items)
5. [Expressions and precedence](#5-expressions-and-precedence)
6. [Enums and pattern matching](#6-enums-and-pattern-matching)
7. [Composition rules (the big section the user asked about)](#7-composition-rules-the-big-section-the-user-asked-about)
8. [Variable scoping, hiding, inheritance](#8-variable-scoping-hiding-inheritance)
9. [Type semantics](#9-type-semantics)
10. [FSM semantics](#10-fsm-semantics)
11. [FTI — Foreign Type Interface](#11-fti--foreign-type-interface)
12. [Operators and symbols (complete list)](#12-operators-and-symbols-complete-list)
13. [Auto-injection passes](#13-auto-injection-passes-pre-translation)
14. [Discrepancies the bootstrap needs to fix](#14-discrepancies-the-bootstrap-needs-to-fix-consolidated)

---

## 1. Lexical structure

Source: `runtime/src/lexer.rs`.

### Encoding and characters

UTF-8 input. Identifiers are ASCII-only:

```rust
fn is_ident_start(c: char) -> bool { c.is_ascii_alphabetic() || c == '_' }
fn is_ident_continue(c: char) -> bool { c.is_ascii_alphanumeric() || c == '_' }
```

**DISCREPANCY** — `tiny-runtime/src/parser.py` uses `isalpha()` which permits
Unicode identifiers. The Rust reference is ASCII-only. Either is reasonable;
flagging the difference.

### Keywords

The word keywords (recognized by `keyword_or_ident` in `lexer.rs:343`):

```
schema  claim  type  subclaim  fsm  external  enum
match  matches  import  in  true  false  mapsto
```

Note: **`schema` is deprecated** but still parses (treated as `type`).
`mapsto` is the ASCII alternative to `↦`.

**DISCREPANCY** — `tiny-runtime` recognises a different set:
`{claim, fsm, fti, type, match, mod, true, false}`. It is missing
`schema, subclaim, external, enum, matches, import, in, mapsto` and has
`fti` and `mod` that the Rust reference does not.

* `fti` in tiny-runtime is its own decl keyword. **In the Rust reference,
  FTI is not a keyword.** FTI types are declared with `external type` /
  `external fsm`. See §11.
* `mod` (modulo) is in tiny-runtime; the Rust reference has only `+ - * /`
  (no modulo operator).
* `schema` is the deprecated synonym for `type` and `subclaim` is
  load-bearing; both are missing from tiny-runtime.

### Literals

* `Int` — `[0-9]+`. `i64`.
* `Real` — `digits.digits`. Only consumes the `.` if followed by a digit
  (so `3.foo` → `Int(3) Dot Ident("foo")`).
* `Str` — `"..."`. Escapes: `\"`, `\\`, `\n`, `\t`. No embedded raw newlines.
* `true` / `false` — `Bool`.

### Comments

Line comments only: `-- to end of line`.

**DISCREPANCY** — tiny-runtime uses `;` for line comments. Reference uses `--`.

### Operators (single token, ASCII and Unicode)

| Token         | Unicode | ASCII alt   | Meaning                                  |
|---------------|---------|-------------|------------------------------------------|
| `In`          | `∈`     | `in`        | set-membership                           |
| `NotIn`       | `∉`     | —           | desugars to `¬(lhs ∈ rhs)`               |
| `ContainsRev` | `∋`     | —           | desugars to `rhs ∈ lhs`                  |
| `And`         | `∧`     | —           | logical AND                              |
| `Or`          | `∨`     | —           | logical OR                               |
| `Not`         | `¬`     | —           | logical NOT                              |
| `Implies`     | `⇒`     | `=>`        | implication                              |
| `Le`          | `≤`     | `<=`        | less-or-equal                            |
| `Ge`          | `≥`     | `>=`        | greater-or-equal                         |
| `Neq`         | `≠`     | `!=`        | not-equal                                |
| `ForAll`      | `∀`     | —           | universal quantifier                     |
| `Exists`      | `∃`     | —           | existential quantifier                   |
| `MapsTo`      | `↦`     | `mapsto`    | named-arg / slot-rename                  |
| `LSeq` / `RSeq` | `⟨` / `⟩` | —       | sequence-literal delimiters              |
| `Eq`          | `=`     | —           | equality / membership-assert             |
| `Lt`, `Gt`    | `<`, `>`| —           | comparisons (and generic-args brackets)  |
| `Plus`, `Minus`, `Star`, `Slash` | `+ - * /` | — | arithmetic                  |
| `PlusPlus`    | `++`    | —           | Seq / String concatenation               |
| `LParen` `RParen` `LBrace` `RBrace` `LBracket` `RBracket` | | | grouping, sets, indexing |
| `Hash`        | `#`     | —           | cardinality (prefix)                     |
| `Comma`       | `,`     | —           | separator                                |
| `Pipe`        | `|`     | —           | enum variant separator                   |
| `Question`    | `?`     | —           | ternary conditional                      |
| `DotDot`      | `..`    | —           | range / passthrough composition          |
| `Dot`         | `.`     | —           | field access                             |
| `Colon`       | `:`     | —           | quantifier body separator                |

**There is no `mod` operator** in the Rust reference. The `mod` keyword in
tiny-runtime is non-standard.

### Indentation and layout

Significant whitespace (Python-style):

* Logical lines end on `\n` → emits `Newline`.
* The next non-blank line emits `Indent(n)` where `n` is the count of
  leading spaces (tabs count as 4).
* **Inside `( [ { ⟨ ` groups, newlines are silently consumed** —
  expressions can span lines. (Lexer tracks `paren_depth`.)
* No DEDENT token: the parser inspects `Indent(n)` peeks to decide
  block boundaries.
* The very first content of the file emits `Indent(0)`.

**DISCREPANCY** — tiny-runtime emits `INDENT` / `DEDENT` pairs (Python-style),
the Rust reference emits `Indent(n)` on every logical line. The parsers handle
blocks differently; the result is similar but block-boundary logic in tiny-runtime
is more brittle to indentation drift.

---

## 2. Top-level declarations

Source: `runtime/src/parser/program.rs`.

```bnf
program       ::= (top_decl)*
top_decl      ::= import_decl | enum_decl | schema_decl
import_decl   ::= "import" StringLit
schema_decl   ::= ["external"] ("schema" | "claim" | "type" | "fsm") IDENT
                  [type_params] [first_line_params] [body]
enum_decl     ::= "enum" IDENT "=" enum_variants
type_params   ::= "<" IDENT ("," IDENT)* ">"
first_line_params ::= "(" param_group ("," param_group)* ")"
                    | "(" ")"
param_group   ::= IDENT ("," IDENT)* "∈" type_name
body          ::= NEWLINE INDENT(n) body_item+ (sharing indent n)
```

* **`external` modifier**: legal on `type`, `claim`, `fsm`; **rejected** on
  `schema`. Used for FTI: only external schemas may emit FFI / LibCall effects.
* All four schema keywords (`schema`, `claim`, `type`, `fsm`) produce the same
  AST node `SchemaDecl` with a different `Keyword` tag.

---

## 3. Schemas: type / claim / schema / fsm

### Keyword conventions (semantic, not grammatical)

From CLAUDE.md (the authoritative source for the reading contract):

| Keyword     | Use for                                                                                        |
|-------------|------------------------------------------------------------------------------------------------|
| `type`      | A noun — record / struct with local field invariants only                                      |
| `claim`     | A predicate / property / relation across multiple values; reusable constraint module           |
| `schema`    | Deprecated synonym for `type`. Does not appear in human-written code.                          |
| `fsm`       | Finite state machine — auto-instantiated by the multi-FSM scheduler. **Load-bearing keyword.** |
| `subclaim`  | A claim nested in a body. Registered globally at load time but inherits parent's variables.    |

**`fsm` is the SOLE signal of FSM-ness.** There is no shape detection: a
`type` / `claim` whose body "looks like" an FSM is NOT an FSM. This was
explicitly settled (session TT killed shape detection). The implication
matters for `sat_*` / `unsat_*` static tests: they pin `state` to assert
properties but use `claim`, so the scheduler never runs them.

### First-line param list

A compact way to declare leading body memberships:

```evident
type IVec2(x, y ∈ Int)              -- two memberships in one line
fsm Counter(state ∈ S, halt ∈ Bool) -- two state-pair params
```

This is **strictly equivalent** to writing the same memberships at the top of
the body. `param_count` records how many leading body items came from the
first-line list, because composition (positional invocation) targets exactly
those.

* Inside the parens, types may carry generic args (`Edge<Rect>`) or be
  compound (`Seq(Int)`, `Set(T)`). Pin clauses inside the parens are **not**
  supported (COUNTEREXAMPLES #4): `Timer (interval_ms ↦ 50)` must be moved
  into the body.

### Generic schemas

```evident
type Edge<T>(from, to ∈ T)
claim Toposort<T>
    items ∈ Seq(T)
    edges ∈ Seq(Edge<T>)
    sorted ∈ Seq(T)
```

* Capitalized identifier names type parameters; they live in a separate
  namespace from value identifiers.
* Bare `Edge` is a *template*; only concrete monomorphizations (`Edge<Rect>`,
  `Edge<Effect>`) are translated. Templates are skipped by `check` /
  scheduling.
* Generic instantiation: each use of `Edge<Rect>` produces a concrete schema
  whose body has `T` substituted. Done iteratively to fixpoint by
  `portable::generics::monomorphize_generics` at load time.
* **No type-argument inference**: `Toposort(...)` errors; must write
  `Toposort<Rect>(...)`. (See `docs/design/generics.md`.)

### Body

The body is a list of `BodyItem`s sharing one indent. The grammar is
order-independent within the body — constraints are *simultaneous*, not
sequential.

---

## 4. Body items

Source: `runtime/src/parser/body_item.rs`, AST: `core/ast.rs::BodyItem`.

There are six kinds:

| Variant                  | Surface syntax                                | Meaning |
|--------------------------|-----------------------------------------------|---------|
| `Membership`             | `x ∈ Type` (optionally `(pins)`)              | Declare a typed variable. |
| `Passthrough(String)`    | `..ClaimName`                                 | Trait composition — see §7. |
| `SubclaimDecl(SchemaDecl)` | `subclaim Name … (indented body)`           | Register a top-level schema; share parent's vars. |
| `ClaimCall {name, mappings}` | `ClaimName(slot ↦ value, …)`              | Explicit-mapping invocation. |
| `Constraint(Expr)`       | any expression                                 | An assertion. |
| `HaltsWithin` (vestigial) | (no longer parseable)                        | Removed surface; kept for AST encoder mirror. |

### Membership pins

`name ∈ Type` with optional pin clause `(pins)`:

```evident
pos ∈ IVec2 (x ↦ 5, y ↦ 7)        -- Pins::Named
pos ∈ IVec2(5, 7)                  -- Pins::Positional
sky ∈ Color                        -- Pins::None
```

Named pins are partial (omit fields to leave free); positional pins must be
≤ field count, pinning the leading fields in declaration order.

### Chained-membership desugaring

A line like `0 < x ∈ Int < 5` is detected by `try_parse_chained_membership`
and desugars to:

```
x ∈ Int
0 < x
x < 5
```

The variable name must be a bare identifier (no field access). Multi-name
form: `x, y, z ∈ Int < 5` declares 3 vars each bounded.

### Constraint-vs-ClaimCall ambiguity

`Foo(a ↦ b)` (with `↦` as the second token inside the parens) parses as a
`ClaimCall`. `Foo(a, b)` parses as a `Call` expression in `Constraint`
position. The disambiguator is whether the second token after `(` is `MapsTo`.

### `subclaim`

```evident
subclaim Name(optional_params)
    -- body shares the parent's variables by name
    ...
```

* `subclaim` is **registered as a top-level schema** at load time (see
  `runtime/src/runtime/validate.rs::register_subclaims`). So
  `recv.subclaim_name(args)` and bare `subclaim_name` references work
  anywhere.
* But: the subclaim *body* sees the parent's variables. When called via
  receiver-prefix dispatch (`recv.subclaim_name(args)`), the receiver's
  fields are mirrored as bare names inside the subclaim body.
* Subclaim-internal vars are fresh per invocation (per-call Z3 suffix) so
  they don't collide.

---

## 5. Expressions and precedence

Source: `runtime/src/parser/exprs.rs`.

### Precedence ladder (low → high)

| Level     | Operators                                  | Notes |
|-----------|--------------------------------------------|-------|
| 1 (lowest) | `∀ vars ∈ range : body`, `∃ vars ∈ range : body` | Quantifier; block form `: NEWLINE INDENT body` AND-joins lines. |
| 2         | `⇒`                                        | Implication. Right-assoc. Block form `⇒ NEWLINE INDENT body` AND-joins consequents. **TIGHTER THAN `∧` (footgun, see CLAUDE.md).** |
| 3         | `? :`                                       | Ternary. Right-assoc. Between `⇒` and `∨` in precedence. |
| 4         | `∨`                                         | Left-assoc. |
| 5         | `∧`                                         | Left-assoc. |
| 6         | `=` `≠` `<` `≤` `>` `≥` `∈` `∉` `∋` `matches` | Comparison. Single use; chained comparisons (`20 ≤ x ≤ 740`) AND-combine pairwise. |
| 7         | `+` `-` `++`                                | Left-assoc. `++` is Seq/String concat. |
| 8         | `*` `/`                                     | Left-assoc. |
| 9         | unary `¬`, unary `-`, prefix `#`            | `#expr` = cardinality. |
| 10 (highest) | `expr[idx]`, `expr.field`                | Postfix index + field-access. |

**Precedence footguns** (called out in CLAUDE.md):

* `A ⇒ B ∧ C` parses as `(A ⇒ B) ∧ C` — opposite of standard math. Parenthesize.
* `in_box = x ≤ 5 ∧ y ≤ 7` parses as `((in_box = x) ≤ 5) ∧ (y ≤ 7)` — `=` is
  tighter than `∧`. Wrap the RHS.

### Atoms

* `Int`, `Real`, `Str`, `true`, `false`
* `match` expression (see §6)
* `IDENT` (with optional chained `.field` parts and optional `<T>` generic
  args followed by `(args)` for generic constructors)
* `name(arg, …)` — `Call` expression. Used for builtins (`coindexed`, `edges`),
  record literals (`IVec2(380, 280)`), claim calls, enum-variant constructors.
* `(expr)` — grouping
* `(e1, e2, …)` — `Tuple` (≥ 2 elements; one element is just grouping)
* `{e1, e2, …}` — `SetLit`. Only valid as RHS of `∈`. Not a first-class set.
* `{lo..hi}` — `Range`. Integer range, valid only as quantifier bound.
* `{}` — empty `SetLit`.
* `⟨e1, e2, …⟩` — `SeqLit`. Index-pinned sequence literal. `⟨⟩` = empty.

### Quantifiers

```evident
∀ x ∈ seq : x > 0                       -- single-var
∀ (a, b) ∈ coindexed(seqA, seqB) : ...  -- tuple-binding (≥ 2 names)
∀ x ∈ range :
    body_line_1                          -- block form; AND-joins
    body_line_2
```

`coindexed(seqA, seqB, …)` and `edges(seq)` are recognized n-ary iteration
builtins. Both require pinned lengths (see CLAUDE.md "N-arity sequence iteration").

### Ternary

`cond ? a : b` — right-associative; lowers to Z3 `ite`. Both branches must
have the same sort.

### Implication

`A ⇒ B`. Right-assoc. Block form:

```evident
parsed.verb = Look ⇒
    StateTurn
    LookAction
```

The block form AND-joins the indented consequents, sidestepping the
`⇒ B ∧ C` precedence footgun.

`A ⟸ B` (reverse implication, `U+27F8` — **NOT in the Rust lexer**) is
mentioned in CLAUDE.md as supported for dispatch tables but I cannot find
the token wired into `lexer.rs`. CLAUDE.md describes it; either the lexer is
missing it or it's expressed differently. **Investigate before relying on
`⟸` in source.**

### Indexing and field access

* `seq[i]` → `Index(seq, i)` — lowers to Z3 `nth`.
* `recv.field` → if `recv` is a bare ident, atoms collapses the dotted chain
  into one identifier string (`win.renderer.set_draw_color`); else,
  postfix produces `Field(recv, name)`. This matters: `pts[0].x` is
  `Field(Index(pts, 0), "x")`.
* `#expr` → `Cardinality(expr)` — Length for `Seq`.

---

## 6. Enums and pattern matching

Source: `runtime/src/parser/program.rs::parse_enum_decl`, AST: `EnumDecl`,
`EnumVariant`, `EnumField`.

```evident
enum Color = Red | Green | Blue
enum Result = Ok(Int) | Err(String)
enum LL = Nil | Cons(Int, LL)            -- self-recursive
enum A = X(B) ; enum B = Y(A)             -- mutual; uses Z3 create_datatypes
```

Multi-line form:

```evident
enum Verb =
    Look
    | Go
    | Take
```

* Variant names are **globally unique** across all enums (load-time failure
  on collision; see COUNTEREXAMPLES #3).
* Payload fields are auto-named `f0, f1, …`.
* A variant with no payload uses bare `Name`; with payload, `Name(T1, T2, …)`.
  `Name()` is rejected — drop the parens for nullary.

### Match expressions

```evident
match scrutinee
    Ctor(b)    ⇒ body
    Other(x, y) ⇒ other_body
    _          ⇒ fallback
```

* Arms are indent-delimited (deeper than the `match` line).
* All arms must produce values of the same Z3 sort.
* Exhaustive (or has `_` wildcard).
* Lowers to nested ITE.
* Patterns recurse: `Ctor(Bind(s), Wildcard)` is allowed.

  **COUNTEREXAMPLES #2** — nested constructor patterns
  (`ResCons(_, ResCons(r, _))`) don't currently parse. Pattern recursion
  is implemented (`parse_match_pattern` is recursive), but there is some
  bug in how patterns inside patterns are handled. Open gap.

### Pattern grammar

```
pattern ::= "_"                  → Wildcard
          | lowercase IDENT      → Bind(name)
          | Uppercase IDENT      → Ctor(name, [])  (nullary)
          | IDENT "(" pattern ("," pattern)* ")"  → Ctor(name, binds)
```

The Bind / Ctor distinction is by *capitalization*: lowercase starts a
binder, uppercase a constructor.

### Recognizer expression: `e matches Pattern`

```evident
result matches StringResult(_)
```

Bool expression — true iff `e`'s variant tag matches the pattern. Payload
bindings are *ignored* (purely a tag check). Use `match` to bind payloads,
or `e = Ctor(7)` for literal-payload equality.

### Sequence literal `⟨…⟩`

Distinct from `{…}` (set literal): `⟨a, b, c⟩` lowers to
`Cons(a, Cons(b, Cons(c, Nil)))` against a `Cons`/`Nil`-shaped enum
(EffectList, ResultList, ArgList, user LinkedList). The LHS hint disambiguates.

### `++` Seq concatenation

`a ++ b ++ ⟨c⟩` is a load-time desugaring (`desugar_seq_concat`), flattening
into one `SeqLit`. Operands must be statically resolvable — `SeqLit` literals
or identifiers that name a body-level `name = ⟨…⟩` binding. Opaque Seq vars
(from a claim invocation) don't flatten and produce the standard "couldn't
translate to Bool" error.

---

## 7. Composition rules (the big section the user asked about)

This is the section that motivated the spec. The user identified three modes
of claim composition. I've verified all three and found additional ones.

### 7.1 Implicit by-name (names-match shorthand)

Default composition mode. Reference any claim by name; outer-scope variables
with matching names are automatically identified with the claim's variables.

```evident
claim within_budget
    assignments ∈ Set(Assignment)
    budget      ∈ Nat
    -- ... constraints ...

claim valid_team
    assignments ∈ Set(Assignment)
    budget      ∈ Nat
    within_budget                       -- 'assignments' and 'budget' flow by name
```

* Parser: `within_budget` is parsed as an `Expr::Identifier` inside a
  `BodyItem::Constraint`. The inliner (`inline_body_items_guarded`) checks
  whether the identifier names a schema and dispatches via
  `inline_guarded_claim` (when wrapped in `⇒`) or via the names-match path.
* Where the resolution happens: in the inliner's `inline_claim_call` style
  fallback for plain-identifier-of-schema. All variables of the claim with
  matching names in the caller's env are wired up; unmatched ones get fresh
  Z3 consts with per-call suffixes (`{claim}__{var}__call{N}`).
* **Self-reference (recursion):** the runtime tracks visited claims with a
  depth counter (`runtime/src/translate/inline/recursion.rs`). Recursion is
  *bounded* by the `try_enter` / `exit_frame` machinery, with per-call var
  isolation so recursive calls get distinct Z3 consts.

### 7.2 Explicit ↦ binding (named mapping)

`ClaimName(slot ↦ value, …)`. Disambiguated from a record literal by the
`↦` as the second token inside the parens.

```evident
claim manage_event
    assignments ∈ Set(Assignment)
    Conference.valid (schedule ↦ assignments)   -- rename to match
```

* Multiple mappings are comma-separated.
* `mapsto` is the ASCII alternative to `↦`.
* The runtime's `inline_claim_call` resolves each `slot ↦ value` mapping,
  inserts the resolved value into the inner env under the slot name, then
  recurses into the claim's body.

### 7.3 Positional invocation

`ClaimName(arg1, arg2, …)` — looks like a function call. Args bind to the
claim's first N **membership** body items (i.e. first-line params expand
into memberships, and any body memberships after them also count for slot
order).

```evident
claim Distinct(s ∈ Seq, n ∈ Nat)
    ...

claim my_problem
    items ∈ Seq(Int)
    Distinct(items, 8)              -- positional
```

* Tuple-as-record coercion: when a slot's type is a known schema (record),
  a tuple arg `(a, b)` is auto-promoted to `Type(a, b)`.

### 7.4 Variable pulling: `..ClaimName`

The user called this "variable pulling — brings all of the called claim's
variables into the parent's top-level scope."

```evident
type main
    ..LineReader      -- adds line, line_ready, src.* directly into scope
    ..LineWriter      -- adds line_out, dst.* directly into scope
```

* Parser: a `..` prefix produces `BodyItem::Passthrough(name)`.
* Inliner (`walk.rs`): a Passthrough recursively inlines the claim's body
  AS IF the body items were the parent's own. Memberships add new variables
  to the parent's env; constraints fire in the parent's context.
* The body items run in the **parent's** env (not a fresh inner env). So
  Memberships truly add to the parent. This is the semantic difference vs.
  names-match: names-match clones the env; `..` *unifies* the env.

**Disambiguator from the docs (CLAUDE.md):** use `..LineReader` when its
fields ARE fields of the current type (no prefix). Use `reader ∈ LineReader`
when you want a sub-object (`reader.line`).

### 7.5 Subclaim-of-type dispatch (receiver-prefix)

A `subclaim` declared inside `type T`'s body becomes invocable as
`recv.subclaim_name(args)` on any `recv ∈ T`:

```evident
type SDL_Window
    renderer ∈ Renderer
    -- ...
    subclaim set_draw_color
        color ∈ Color
        eff   ∈ Effect
        -- body uses 'renderer' bare; the inliner mirrors recv.renderer → renderer

claim app
    win ∈ SDL_Window
    win.set_draw_color((220, 40, 40, 255), eff)
```

* Implementation: `inline_subschema_call` clones the env, walks `recv.*` keys,
  re-binds each as the bare-name key. Then runs the subclaim body.
* This is implemented as a separate dispatch path in `walk.rs` (priority
  over generic claim-call dispatch).

### 7.6 Guarded claim invocation: `cond ⇒ ClaimName`

`cond ⇒ ClaimName` inlines the claim's body, wrapping each *constraint* in
`cond ⇒ …`. **Declarations (Memberships) fire unconditionally.** Composes
with names-match.

```evident
type main(state, state_next ∈ GameState)
    state.step = 0 ⇒ InitGameState   -- run Init's constraints only on step 0
```

Implementation: `inline_guarded_claim` in `translate/inline/calls.rs`.
Composes with outer guards (`compose_guards`).

### 7.7 Reverse implication: `A ⟸ B`

CLAUDE.md describes `⟸` for dispatch tables:

```evident
GoAction ⟸ verb = Go        -- "GoAction applies when verb = Go"
```

It's syntactic sugar that swaps the implication. **However** I cannot find
`⟸` (`U+27F8`) wired into `lexer.rs`. CLAUDE.md describes it but the
runtime doesn't accept it as a token. Either the docs are aspirational or
there's a token-mapping I missed. **Treat with suspicion.**

### 7.8 Tuple-in-claim: `(args) ∈ ClaimName`

A relational form of positional invocation:

```evident
(items, 8) ∈ Distinct
```

Parses to `Constraint(InExpr(Tuple(items, 8), Identifier("Distinct")))`.
Inliner: `inline_tuple_in_claim` treats the tuple as a positional arg list,
runs the claim body with each tuple element bound to the corresponding slot.

### Summary table

| Mode                          | Surface                              | Env behavior                            |
|-------------------------------|--------------------------------------|------------------------------------------|
| Names-match (bare invocation) | `ClaimName`                          | Fresh inner env, names-match wires vars  |
| Named mapping                 | `ClaimName(slot ↦ val, …)`           | Fresh inner env, explicit slot binding  |
| Positional                    | `ClaimName(val1, val2, …)`           | Fresh inner env, args bind to first N memberships |
| Tuple-in                      | `(val1, val2) ∈ ClaimName`           | Same as positional, relational notation |
| Receiver-prefix subclaim      | `recv.subclaim(args)`                | Fresh inner env, recv.* mirrored bare   |
| Guarded                       | `cond ⇒ ClaimName`                   | Fresh inner env + every constraint wrapped in cond |
| Passthrough                   | `..ClaimName`                        | **Parent's env — vars are pulled up**   |

### Name-collision rules

* **Inside fresh-env modes:** the named/positional/tuple mapping wins for
  the slot. Unmapped vars of the claim get fresh Z3 consts (per-call suffix).
* **Inside passthrough:** memberships are added to the parent env. If a
  name already exists in the parent env, the `inline_membership` path
  skips the duplicate declaration (`if !env.contains_key(name)`).
* **Recursion:** `try_enter` increments a depth counter per claim name; if
  the same claim is already being processed, `try_enter` returns `None` and
  the call is skipped (bounded recursion guard).

### Are claim references first-class?

**No.** A claim name is an `Expr::Identifier` and a use-site is a body
item. Claims cannot be assigned to variables, passed around, or used in
higher-order constructs (no `claim foo = bar`, no `apply(claim_ref, args)`).
The language is first-order over claims.

This was confirmed by the `generics.md` design doc which explicitly notes:
"no generic functions / lambdas."

---

## 8. Variable scoping, hiding, inheritance

### Scoping rules (from CLAUDE.md "Key Invariants")

* Variables declared inside a schema body are **local to that schema's
  query** — i.e. the names live only within that schema's translation.
* A sub-schema membership `task ∈ Task` expands into **per-field Z3 leaves**
  (`task.id`, `task.duration`, …). The bare `task` variable is never stored
  in env; only the leaves are.
* Type names can shadow as variable names without conflict — they live in
  separate namespaces.
* Subclaim-internal vars are fresh per-invocation (per-call Z3 const
  suffix); they're not visible to the parent.

### "Variable hiding"

The user mentioned hiding. There are several mechanisms that effectively
hide variables:

1. **`_`-prefix convention** (from `grammar-rules.md`): names like `_partial`
   are body-internal implementation scaffolding. The convention is purely
   stylistic — the runtime doesn't enforce hiding, but the convention
   signals "this is not part of the claim's interface."

2. **Body-vs-first-line distinction**: variables declared in the first-line
   params are the claim's interface. Body-level memberships are internal
   helpers and get fresh consts per call (via `isolate_helper_locals` at
   `runtime/src/translate/inline/recursion.rs`).

3. **Subclaim internals**: any membership declared inside a `subclaim` is
   per-call fresh.

### "Variable inheritance"

1. **Subclaims inherit the parent's variables** — see test_38 / the
   subclaim docs in CLAUDE.md. A subclaim's body sees the parent's vars by
   name without parameters.
2. **Receiver-prefix dispatch inherits the receiver's fields** as bare
   names inside the subclaim body.
3. **Passthrough composition** (`..`) inherits everything from the
   referenced claim.
4. **Type-body constraints inherit per-instance** — when you write
   `t ∈ MyType` and `MyType`'s body has constraints that reference its own
   fields, those constraints are inlined with `t.` prefix.
5. **`Seq(SomeType)` per-element inheritance**: if `xs ∈ Seq(Edge)` and
   `#xs = 3`, then `Edge`'s body constraints are instantiated 3 times,
   substituting `xs[0]`, `xs[1]`, `xs[2]`. See `inline_membership` in
   `runtime/src/translate/inline/membership.rs`.

### Types compose, no inheritance hierarchy

Evident has **no class-based inheritance**. Composition mechanisms (above)
are the only way to share structure. This is Go's "composition-over-inheritance"
philosophy:

* A type can contain another via `field ∈ OtherType` (sub-object access).
* A type can flat-mixin via `..OtherType` (fields lift in).
* A claim can be a constraint module mixed into a type.
* Constraints can be inherited per-element across a sequence.

No `extends`, no `is_a`, no subtype polymorphism. Generic types
(`Edge<T>`) are parametric polymorphism only.

---

## 9. Type semantics

### Type-body constraints

Yes, types can have constraints (see CLAUDE.md "type" entry):

```evident
type DateRange
    start ∈ Date
    end   ∈ Date
    start ≤ end          -- local invariant
```

The constraint applies to every instance of `DateRange`. When `dr ∈ DateRange`
is declared, `dr.start ≤ dr.end` is asserted (prefix substitution via
`rewrite_idents_with_prefix`).

**Rule from CLAUDE.md:** constraints referencing **external** data cannot
live in a type body — they'd be silently dropped because the sub-env only
contains the type's own fields. Move such constraints to a claim where the
global fact is in scope.

### Refined types (subset types)

You can build a refined type by naming claims inside a type body:

```evident
type ValidSchedule
    slots   ∈ Seq(TimeSlot)
    budget  ∈ Nat
    no_conflicts     -- claim; 'slots' matches by name
    within_budget    -- claim; 'budget' matches by name
```

Every instance of `ValidSchedule` satisfies the named claims (constraints
of `no_conflicts` and `within_budget` are inlined per instance).

### Generic types

Already covered in §3.

* Generic templates aren't queryable. Use sites produce concrete schemas
  via monomorphization.
* Identity in generic claims is by Z3 value equality on `T`. Two
  structurally-equal `Rect`s are the same vertex.
* Type-parameter names are scoped to the declaring schema. No higher-kinded
  types.

### `x ∈ MyType` semantics

When `x ∈ MyType` is declared:

1. `declare_var` registers `x.field_i` for each field of `MyType`. The
   bare `x` is not in env.
2. Type body constraints are inherited, prefixed with `x.`.
3. Optional pins (`x ∈ MyType(v1, v2)` or `x ∈ MyType (slot ↦ v)`) emit
   `x.field = v` equalities.
4. If `MyType` is `Seq(Inner)`, length is read from env; per-element
   constraint inheritance fires for each index.

---

## 10. FSM semantics

### State-pair convention

The classical form:

```evident
fsm Counter(state, state_next ∈ S)
    state_next = match state ...
```

`state` is this tick's value; `state_next` is the next tick's. The body
asserts the transition relation.

### Terse `_var` form (preferred)

```evident
fsm Counter(state ∈ S)
    state = match _state ...        -- _state = previous tick
```

* `_X` is the previous-tick value of `X`.
* The runtime auto-injects `is_first_tick ∈ Bool` whenever any `_var` is
  referenced.
* `unify_state_syntax` (`runtime/src/runtime/desugar.rs`) rewrites the
  terse form to the legacy `state, state_next ∈ S` pair before translation.

### Auto-injected slots

If a body uses these names without declaring them, the loader auto-injects
the membership:

* `last_results ∈ Seq(Result)` — outcomes of the previous tick's effects.
* `effects ∈ Seq(Effect)` — this tick's emitted effects.
* `world ∈ World` / `world_next ∈ World` — shared multi-FSM state (when the
  program has a `type World`).
* `is_first_tick ∈ Bool` — true on tick 0.

### Unified `_world.X` / `world.X` syntax

```evident
fsm game(world ∈ World)
    world.pos = _world.pos + ...      -- _world.X reads prev, world.X writes
```

The runtime rewrites `_world.X` → `world.X` and `world.X` → `world_next.X`,
matching the legacy writer pattern.

### Multi-FSM composition

Each top-level `fsm` schema is run as an **independent FSM** by the
multi-FSM scheduler (`runtime/src/effect_loop/`).

* **Subscription-driven scheduling**: an FSM ticks only when one of its
  inputs changes (world read-set, effect self-feedback, state self-feedback,
  or bootstrap on tick 0).
* **Synchronous-language semantics** within a tick: all FSMs' constraints
  hold simultaneously; Z3 finds an assignment satisfying all of them. No
  intra-tick ordering. Cross-FSM dependencies are shared world variables;
  Z3 propagates automatically.
* **Single-writer-per-field**: at most one FSM writes any given world field
  (enforced at load).
* **Halt** is implicit: no FSM scheduled in a tick → halt. `Effect::Exit(code)`
  graceful end-of-tick exit.

### Embedded FSM constraints

Within a parent schema, a 2-arg call to an `fsm`-keyword schema is rewritten
to `RunFsm`:

```evident
fsm decrement(count ∈ Int, halt ∈ Bool)
    count = _count - 1
    halt  = (_count ≤ 0)

claim sat_settles_to_zero
    final ∈ Int
    decrement(50, final)        -- F(seed, fsm_state) — embed
    final = 0
```

* The rewrite is done by `lower_fsm_application` after the program is loaded
  (full schema table known).
* `final` is bound to F's *settled state* — the state at the first tick
  where `halt = true`.
* The parent may further constrain `final` (e.g. `final = 0`). The constraint
  is on the settled state; if F can't settle, UNSAT.
* The old surfaces `run(F, init)` and `halts_within(F, N)` are **REMOVED**.
  Both are subsumed by the constraint embedding.

### Effects from embedded FSMs

A child FSM run via the embed surface may emit effects. The effects are
**captured during the child's solve**, not dispatched. They percolate to
the parent and the parent dispatches them once (test_38).

---

## 11. FTI — Foreign Type Interface

**Critical correction for the user**: in the Rust runtime, **`fti` is NOT
a keyword**. FTI types are declared with `external type` or `external fsm`.

```evident
external type SDL_Window
    title       ∈ String
    size        ∈ IVec2
    fullscreen  ∈ Bool
    -- bridge plugin observes user writes, mirrors window state

external type Timer
    interval_ms ∈ Int
    tick_count  ∈ Int

external fsm StdinSource
    stdin_line ∈ String
    stdin_seq  ∈ Int
```

* `external` is a parse-time modifier on the schema keyword.
  `external schema` is rejected.
* Only `external` schemas may emit FFI / LibCall effects (validated by
  `enforce_external_only` at load).
* The runtime's `fti.rs` registry maps type names to install / bridge
  implementations (`runtime/src/event_sources/`).
* Bridge plugins observe user declarations of the type and materialize
  the C-side resource. See `docs/design/foreign-type-interface.md` for
  the design direction (it's noted as design-only in some places but
  parts are in production: SDL_Window, GL_Program, Timer, FrameClock,
  StdinSource, etc., all live in `stdlib/runtime.ev`).

### Install steps

The persistent-handle bridges use an `install ∈ Seq(InstallStep)` body member:

```evident
external type Hostname
    name ∈ String
    install ∈ Seq(InstallStep) = ⟨Bind("name", ShellRun("hostname"))⟩
```

```evident
enum InstallStep =
    Run(Effect)                -- fire, discard result
    Bind(String, Effect)       -- fire, capture into named field
```

`ArgPriorResult(N)` inside an effect refers to the Nth prior step's result
within an install Seq.

**DISCREPANCY** — tiny-runtime's `fti F(T1, T2) … body …` form does not
match the reference. It looks like an alternative design that diverged.
The reference uses `external type Name (fields …)` with no type params at
the schema head (generic FTIs use `<T>` brackets like other generics).

### Namespace mangling

The reference doesn't do source-level namespace mangling of FTI types. The
type lives at its own name (e.g. `SDL_Window`), and the bridge plugin
resolves by name through `runtime/src/fti.rs`. Field access on an instance
uses the normal `recv.field` mechanism. The Rust-side per-FSM view does
prefix-strip pin keys (`win.title` → `title`) when handing off to the
bridge, but that's internal.

### Libcall threading

The runtime's `Effect::LibCall(library, symbol, signature, args)` performs
**cached dlopen + dlsym** at first use. Each call returns an `EffectResult`
typed by the C-return convention. Handles (open libraries, malloc'd
buffers, dlsym'd functions) live in a `HandleRegistry` keyed by `u64`.

* `Effect::FFIOpen / FFILookup / FFICall / CloseHandle` — explicit handle
  threading.
* `Effect::LibCall` — one-shot, cached internally.

This is in `runtime/src/effect_dispatch.rs` and `runtime/src/ffi.rs`.

---

## 12. Operators and symbols (complete list)

### Comparison

| ASCII | Unicode | Token name |
|---|---|---|
| `=` | — | `Eq` |
| `!=` | `≠` (U+2260) | `Neq` |
| `<` | — | `Lt` |
| `<=` | `≤` (U+2264) | `Le` |
| `>` | — | `Gt` |
| `>=` | `≥` (U+2265) | `Ge` |

### Logical

| ASCII | Unicode | Token name |
|---|---|---|
| `=>` | `⇒` (U+21D2) | `Implies` |
| — | `∧` (U+2227) | `And` |
| — | `∨` (U+2228) | `Or` |
| — | `¬` (U+00AC) | `Not` |

### Set theory

| ASCII | Unicode | Token name | Notes |
|---|---|---|---|
| `in` | `∈` (U+2208) | `In` | set membership |
| — | `∉` (U+2209) | `NotIn` | non-membership |
| — | `∋` (U+220B) | `ContainsRev` | reverse membership |
| — | `∀` (U+2200) | `ForAll` | universal quantifier |
| — | `∃` (U+2203) | `Exists` | existential quantifier |

No union / intersection / difference operators in the language — set
literals exist only as RHS of `∈`.

### Arithmetic

| Token | Notes |
|---|---|
| `+` | addition |
| `-` | subtraction; also unary negation |
| `*` | multiplication |
| `/` | division |
| `++` | sequence/string concatenation |
| `#` | cardinality (prefix) |

**No `mod` operator** in the reference. (tiny-runtime has one.)

### Other

| ASCII | Unicode | Token name |
|---|---|---|
| `mapsto` | `↦` (U+21A6) | `MapsTo` |
| — | `⟨` (U+27E8) `⟩` (U+27E9) | `LSeq` / `RSeq` |

### Reserved but unwired

* `⟸` (U+27F8) — reverse implication. Documented in CLAUDE.md but NOT
  in the lexer. Open question.

---

## 13. Auto-injection passes (pre-translation)

Source: `runtime/src/runtime/load.rs::load_source_with_base`.

In order:

1. **`unify_world_syntax`** — rewrites `_world.X` / `world.X` to the
   legacy `world.X` / `world_next.X` pair. Injects `world_next ∈ World`.
   Only for `fsm` (not external).
2. **`unify_state_syntax`** — rewrites the terse `_X` form to the
   legacy `X, X_next ∈ T` pair for any first-line FSM state var. Skipped
   for primitive state vars unless `halt ∈ Bool` is present.
3. **`desugar_seq_concat`** — flattens `a ++ b ++ ⟨c⟩` chains into a
   single `SeqLit` when all operands are static.
4. **`fsm_params`** (`portable/inject.rs`) — adds `last_results`,
   `effects`, possibly `world` slots for `fsm` schemas missing them.
5. **`inject_lhs_eq_types`** — for `X = Expr` body constraints where `X`
   is undeclared, infer `X ∈ T` from RHS shape (enum variant, record
   ctor, field type, binary-op result type).
6. **`prev_tick`** (`portable/inject.rs`) — adds `_X ∈ T` decls and
   `is_first_tick ∈ Bool` when any `_var` is referenced.
7. **`inject_claim_arg_types`** — for fresh positional-arg names used
   ≥ 2 times in the body, infer `X ∈ T` from the called claim's param
   signature.
8. **`enforce_external_only`** — validate that only `external` schemas
   emit FFI effects.
9. **`monomorphize_generics`** — expand `Edge<Rect>` etc. into concrete
   schemas; iterate to fixpoint.
10. **`lower_fsm_application`** — rewrite 2-arg calls to `fsm`-keyword
    schemas into `RunFsm{F, seed}` settled-state bindings.
11. **`validate_run_targets`** — check embedded FSM targets are valid
    (single state pair, etc).

These passes are why so much code "works without declarations" — the
runtime synthesizes them.

---

## 14. Discrepancies the bootstrap needs to fix (consolidated)

This is the actionable list — what `tiny-runtime` currently gets wrong
or is missing relative to the Rust reference.

### Five biggest things tiny-runtime got wrong

1. **`fti` is not a keyword in the reference.** tiny-runtime treats
   `fti F(T1, T2)` as a top-level decl. The reference uses
   `external type Name` and `external fsm Name`. The `external`
   modifier is the load-bearing signal, not a separate keyword.
   tiny-runtime/src/parser.py:144-163.

2. **Comment syntax is `--`, not `;`.** tiny-runtime uses `;` for line
   comments; every existing `.ev` file in `examples/` and `stdlib/` uses
   `--`. Loading any of the reference programs in tiny-runtime would
   fail to lex. tiny-runtime/src/parser.py:65-67.

3. **`subclaim` is missing.** The reference makes `subclaim` a load-time
   registration mechanism with two distinct dispatch modes (names-match
   and receiver-prefix). tiny-runtime has no `subclaim` keyword at all.
   `runtime/src/parser/schema.rs:8-25`.

4. **Composition mechanisms are absent.** tiny-runtime's parser has
   `parse_stmt` covering only "binding (`x ∈ S`)" and "assertion (any
   expr)". It does not implement:
   * `..ClaimName` passthrough — `parser.py:340-348`.
   * `ClaimName(slot ↦ val)` named-mapping invocation.
   * `subclaim Name … body` nested registration.
   * Chained-membership (`0 < x ∈ Int < 5`).
   * `(args) ∈ ClaimName` tuple-in dispatch.
   * `cond ⇒ ClaimName` guarded invocation.
   All of these are essential for non-toy programs.

5. **Quantifiers, ternary, `match` in tiny-runtime use `:` instead of `⇒` for arms.**
   The reference uses `⇒` (or `=>`) as the arm separator in `match`:
   `Pattern ⇒ body`. tiny-runtime uses `=>` only as a single binop on
   the SYMBOL list (`parser.py:24`) — but `parse_match` in tiny-runtime
   reads `expr : NEWLINE INDENT (pattern => body)+`. The colon after
   `match scrutinee` is reference-incompatible: the reference uses
   `match scrutinee NEWLINE INDENT (Pattern ⇒ body)+`. No colon.
   `runtime/src/parser/patterns.rs:8-46` vs `parser.py:467-485`.

### Five biggest things the Rust runtime has that the bootstrap doesn't

1. **Subscription-driven multi-FSM scheduler.** The reference runs
   multiple `fsm` schemas, infers their world read/write-sets statically,
   schedules ticks based on changes. tiny-runtime has only a trampoline
   for one body. Without the scheduler the entire `examples/test_NN_*.ev`
   set is non-runnable. `runtime/src/effect_loop/`.

2. **Generics + monomorphization.** `Edge<T>`, `Toposort<T>`, generic
   `Seq` element types. The reference iterates to fixpoint at load time,
   producing concrete schemas. tiny-runtime has none of this and likely
   needs it for `stdlib/distinct.ev`, `stdlib/sorted.ev`, etc.
   `runtime/src/runtime/generics.rs` (via `portable/`).

3. **Enums with payloads and recursive/mutual recursion.** The reference
   batches enum decls through Z3's `create_datatypes` so forward and
   mutual references resolve in one pass. tiny-runtime's `type` is a
   simple variant list; the reference's `enum` is far richer and is the
   foundation for `Result`, `Effect`, every multi-state FSM.

4. **Auto-injection passes.** A lot of "this just works" in the reference
   is the inject_* / unify_* / desugar_* passes (§13 above) running
   before translation. Without them, simple FSM programs need ~20 extra
   lines of explicit declarations. The biggest perceived "magic" is:
   * `_var` previous-tick decls.
   * `is_first_tick` auto-injection.
   * `world_next` auto-injection.
   * Type inference for `X = RHS`.
   * Type inference for fresh positional-call args.

5. **External effects (FFI, LibCall, Print, ReadLine, Time, etc).** The
   reference has a rich `Effect` enum, an effect dispatcher, a Handle
   registry, libffi marshaling. tiny-runtime has a `libcall` primitive
   but lacks the `Seq(Effect)` / `Seq(Result)` threading machinery and
   the per-tick `effects` / `last_results` slot wiring. Without these,
   `Println`, `Exit`, `ReadLine` don't work.

### Other notable discrepancies (catch-all)

* **Indentation model** — tiny-runtime uses INDENT/DEDENT pairs (Python
  layout); reference emits `Indent(n)` and the parser inspects column
  on every line. Behaviorally similar but different in edge cases.
* **Identifier charset** — tiny-runtime accepts Unicode letters in
  identifiers; reference is ASCII-only.
* **Lexer/identifier dotted parsing** — tiny-runtime tries to be clever
  in the lexer (`QUALIFIED` token for `a.b.c`); reference produces a
  bare `Ident` and stitches dotted names at the parser level (atoms.rs).
* **String escapes** — tiny-runtime supports `\r`, reference does not.
* **No `mod` operator** in the reference.
* **No first-class real type literal handling** in tiny-runtime — it has
  `INTEGER` and `STRING` but no `REAL` distinct from `INTEGER`. The
  reference lexer disambiguates `3.5` (Real) from `3.foo` (Int + Field).
* **No `match`-with-`⇒` arm separator** — see point 5 above.
* **No `external` modifier** — the loaded FTI types (and the install-step
  / bridge mechanism that hangs off them) can't be expressed.
* **No `import`** — tiny-runtime has no module system, while the reference
  resolves `import "path"` with file-relative + cwd + 10-level ancestor
  walk.
* **No `matches` recognizer** — the bool form `e matches Ctor(_, _)`.
* **No `∉` / `∋`** — non-membership and reverse membership.
* **No `∀`/`∃` quantifiers with tuple binding** — `∀ (a, b) ∈
  coindexed(seqA, seqB)`.
* **No range literal as quantifier bound** — `{lo..hi}`.
* **No set literal as RHS of `∈`** — `x ∈ {1, 2, 3}`.
* **No cardinality (`#`)**.
* **No first-line param list for short types** — `type IVec2(x, y ∈ Int)`.
* **No type-use pins** — `pos ∈ IVec2 (x ↦ 5)` and `pos ∈ IVec2(5, 7)`.
* **No generic constructor call** — `Edge<Rect>(a, b)`.

---

## Appendix A: full BNF (consolidated)

```bnf
program       ::= top_decl*
top_decl      ::= import_decl | enum_decl | schema_decl

import_decl   ::= "import" StringLit NEWLINE

enum_decl     ::= "enum" IDENT "=" enum_body
enum_body     ::= enum_variant ("|" enum_variant)*
                | NEWLINE INDENT(n) ["|"] enum_variant
                  (NEWLINE INDENT(n) ["|"] enum_variant)*
enum_variant  ::= IDENT [ "(" enum_field ("," enum_field)* ")" ]
enum_field    ::= IDENT [ "(" enum_field ")" ]    -- recursive compound type names

schema_decl   ::= ["external"] schema_kw IDENT [type_params]
                  [first_line_params] [NEWLINE INDENT body]
schema_kw     ::= "schema" | "claim" | "type" | "fsm"
type_params   ::= "<" IDENT ("," IDENT)* ">"
first_line_params ::= "(" ")" | "(" param_group ("," param_group)* ")"
param_group   ::= IDENT ("," IDENT)* "∈" type_ref
type_ref      ::= IDENT [generic_args]
                | container_head "(" IDENT [generic_args] ")"
container_head ::= "Seq" | "Set" | "Bag" | "Map"
generic_args  ::= "<" type_ref ("," type_ref)* ">"

body          ::= body_item (NEWLINE body_item)*
body_item     ::= passthrough
                | subclaim_decl
                | claim_call_named
                | chained_membership
                | membership
                | constraint

passthrough   ::= ".." IDENT
subclaim_decl ::= "subclaim" IDENT [first_line_params] [NEWLINE INDENT body]
claim_call_named ::= IDENT [generic_args] "(" slot_mapping ("," slot_mapping)* ")"
slot_mapping  ::= IDENT "↦" expr
chained_membership ::= [expr cmp]* IDENT ("," IDENT)* "∈" type_ref [cmp expr]*
membership    ::= IDENT ("," IDENT)* "∈" type_ref [pin_clause]
pin_clause    ::= "(" slot_mapping ("," slot_mapping)* ")"
                | "(" expr ("," expr)* ")"
constraint    ::= expr

cmp           ::= "=" | "≠" | "<" | "≤" | ">" | "≥"

expr          ::= quantifier_expr | implies_expr
quantifier_expr ::= ("∀" | "∃") quantifier_binder "∈" postfix ":" body_or_expr
quantifier_binder ::= IDENT | "(" IDENT ("," IDENT)+ ")"
body_or_expr  ::= NEWLINE INDENT (implies_expr (NEWLINE)*)+      -- AND-joined
                | expr

implies_expr  ::= ternary_expr [ "⇒" (block_implies | implies_expr) ]
block_implies ::= NEWLINE INDENT (implies_expr (NEWLINE)*)+      -- AND-joined

ternary_expr  ::= or_expr [ "?" ternary_expr ":" ternary_expr ]

or_expr       ::= and_expr ("∨" and_expr)*
and_expr      ::= compare_expr ("∧" compare_expr)*

compare_expr  ::= addsub_expr [ matches_or_rel ]
matches_or_rel ::= "matches" pattern
                 | "∈" addsub_expr
                 | "∉" addsub_expr
                 | "∋" addsub_expr
                 | cmp addsub_expr (cmp addsub_expr)*   -- chained AND-combine

addsub_expr   ::= muldiv_expr (("+" | "-" | "++") muldiv_expr)*
muldiv_expr   ::= unary_expr (("*" | "/") unary_expr)*
unary_expr    ::= "¬" unary_expr | "-" unary_expr | "#" unary_expr | postfix_expr

postfix_expr  ::= atom (postfix_op)*
postfix_op    ::= "[" expr "]" | "." IDENT

atom          ::= Int | Real | Str | "true" | "false"
                | match_expr
                | dotted_ident [generic_args] [ "(" arg_list ")" ]
                | "(" expr ")"
                | "(" expr ("," expr)+ ")"          -- Tuple (≥ 2)
                | "{" "}"                            -- empty set
                | "{" expr ".." expr "}"             -- Range
                | "{" expr ("," expr)* "}"           -- SetLit
                | "⟨" "⟩"                            -- empty Seq
                | "⟨" expr ("," expr)* "⟩"           -- SeqLit
dotted_ident  ::= IDENT ("." IDENT)*
arg_list      ::= (expr ("," expr)*)?

match_expr    ::= "match" or_expr NEWLINE INDENT
                  (pattern "⇒" or_expr (NEWLINE)*)+
pattern       ::= "_"                                 -- Wildcard
                | lowercase_IDENT                     -- Bind
                | IDENT [ "(" pattern ("," pattern)* ")" ]   -- Ctor (capitalized)
```

## Appendix B: pointers to the reference implementation

| Concept | File |
|---|---|
| Token list | `runtime/src/lexer.rs` |
| Parser entry | `runtime/src/parser/mod.rs::parse` |
| Program + enums | `runtime/src/parser/program.rs` |
| Schema decl + first-line params | `runtime/src/parser/schema.rs` |
| Body items + chained-membership | `runtime/src/parser/body_item.rs` |
| Type-name parsing | `runtime/src/parser/types.rs` |
| Expressions + precedence | `runtime/src/parser/exprs.rs` |
| Atoms | `runtime/src/parser/atoms.rs` |
| Match patterns | `runtime/src/parser/patterns.rs` |
| AST node types | `runtime/src/core/ast.rs` |
| Load pipeline | `runtime/src/runtime/load.rs` |
| Desugaring (world, state, ++) | `runtime/src/runtime/desugar.rs` |
| Inject (auto-decl, type inference) | `runtime/src/runtime/inject.rs` |
| Generics monomorphization | `runtime/src/portable/generics.rs` (called from load.rs) |
| Embed-FSM lowering | `lower_fsm_application` in `runtime/src/runtime/nested.rs` |
| Inliner entry | `runtime/src/translate/inline/walk.rs` |
| Names-match / positional / mapping | `runtime/src/translate/inline/calls.rs` |
| Subschema dispatch | `runtime/src/translate/inline/subschema.rs` |
| Membership + per-instance inheritance | `runtime/src/translate/inline/membership.rs` |
| Multi-FSM scheduler | `runtime/src/effect_loop/` |
| Effect dispatch | `runtime/src/effect_dispatch.rs` |
| FTI registry | `runtime/src/fti.rs` |
| Stdlib runtime types (Effect, Result, FrameTimer, …) | `stdlib/runtime.ev` |
| Reading contract (interpretation of keywords) | `CLAUDE.md` |
| Composition design | `docs/design/what-we-learned.md` |
| Multi-FSM design | `docs/design/multi-fsm.md`, `docs/design/fsm-subscriptions.md` |
| Schema interface model | `docs/design/schema-interface.md` |
| Counterexamples / known gaps | `examples/COUNTEREXAMPLES.md` |

## Appendix C: things flagged as unclear or possibly broken

* **`⟸` (reverse implication)** — documented in CLAUDE.md but not in
  the lexer. Either it's missing or expressed differently. **Open.**
* **Nested constructor patterns in `match`** — COUNTEREXAMPLES #2 says
  these don't parse, yet `parse_match_pattern` is written recursively.
  Some interaction between match arms and nested patterns is broken.
* **FTI pins in claim signature** — COUNTEREXAMPLES #4. `claim x(t ∈
  Timer (interval_ms ↦ 50))` is a parse error. Easy to overlook when
  authoring.
* **`state_next` for payload first-variants** — COUNTEREXAMPLES #1. The
  runtime can't seed an FSM whose state-enum's first variant has a
  payload. Workaround: prepend a nullary `Start` variant.
* **External + schema combination** — `external schema` is rejected at
  parse time but `external type`, `external claim`, `external fsm` are
  all accepted. The grammar allows the construction; the validator
  blocks one form.

---

## Extension: `fti` (Foreign Type Interface)

> Added on the `tiny-runtime` branch, 2026-06. This is an extension to
> the reference grammar — it does NOT exist in the Rust runtime on
> `main`. There, FTI types are declared with `external type` /
> `external fsm` (see §11). The bootstrap diverged: rather than infer
> "this declaration is foreign" from an `external` modifier, the
> bootstrap exposes `fti` as a dedicated decl keyword so the parser /
> transpiler can route FTI invocations through the FTI inliner without
> a separate validation pass.

### Surface syntax

```
fti_decl ::= "fti" IDENT [type_params] [first_line_params] [body]
```

* `fti Name<T1, T2>(params)` — generic FTI with type parameters in
  angle brackets, first-line params in parens. Both lists are
  optional.
* `fti Name(T1, T2)` — legacy bootstrap shape: bare type identifiers
  in the parens. Kept transitionally for the `prelude/stack.ev` and
  `prelude/queue.ev` files. The parser disambiguates by whether any
  `∈` appears inside the parens.
* Body has the same shape as a `claim` / `fsm` body — memberships,
  constraints, etc.

```evident
fti Stack(T)
    base ∈ Int              -- externally-allocated region start
    contents ∈ Seq(T)       -- logical stack contents
    effects ∈ Seq(Effect)   -- this FTI's own effects channel

    contents = _contents
       ∨ contents = init(_contents)
       ∨ len(contents) = len(_contents) + 1 ∧ init(contents) = _contents

    effects = match is_init
        true  ⇒ ⟨LibCall("__mem__", "mem_alloc", "l(l)",
                         ⟨ArgInt(8192)⟩, "base", "")⟩
        false ⇒ ...
```

### Semantics

Semantically equivalent to `external fsm Name` with these specific
conventions agreed on during the `tiny-runtime` cycle:

1. **State-pair materialization.** Each FTI body membership `x ∈ T`
   produces a state pair (`_x`, `x`) in the host FSM's namespace,
   prefixed by the host variable name: an instance `s ∈ Stack(Int)`
   produces `_s__base / s__base` and `_s__contents / s__contents`.
2. **Tick-0 init via libcall.** On the first tick (`is_init`), the
   FTI's `effects` channel emits libcalls that initialize the
   external system (allocate buffers, open handles). The libcall
   `ok_dest` field pins the return value into the corresponding
   namespaced const.
3. **Subsequent ticks rely on state-pair carry-forward.** The runtime
   threads `s__base` from one tick into the next as `_s__base`
   automatically. The FTI does NOT re-read external state on every
   tick.
4. **The FTI is the sole writer of its foreign state.** The composing
   FSM asserts relations over the FTI's variables (e.g.
   `s.contents = _s.contents ++ ⟨42⟩`); the FTI's body's libcalls
   make the external system match. No other code touches the foreign
   state.
5. **Per-instance effects channel.** An FTI body's `effects`
   membership becomes `<host_var>__effects` after inlining. The
   runtime auto-discovers any const named `effects` or `*_effects` as
   an effects channel and dispatches them independently each tick.

### Inlining

An FTI is NEVER emitted as a standalone schema. Instead, when a
composing FSM declares `host_var ∈ FtiName(T)`, the transpiler:

1. Looks up the FTI in the FTI registry.
2. Substitutes type parameters (T ↦ the user's type argument).
3. Emits state-pair declarations for each FTI body membership under
   the `host_var__` namespace.
4. Emits the FTI's body assertions, with idents rewritten to the
   namespaced form. `LibCall(...)` calls have their `ok_dest` /
   `err_dest` string slots rewritten if they reference FTI-local
   bindings.

The FTI itself contributes no top-level SMT-LIB output.

### Relationship to `external type` / `external fsm`

The reference's `external type Name` is the closest analog. The two
forms differ in:

| Aspect                | Reference `external type` / `external fsm` | Bootstrap `fti` |
|-----------------------|--------------------------------------------|-----------------|
| Routing               | Runtime FTI registry (`runtime/src/fti.rs`)| Transpiler-side inliner |
| Generic params        | `<T>` brackets                             | `<T>` brackets OR legacy `(T)` parens |
| Standalone emission   | Yes (the registered type has its own state)| No (always inlined) |
| Validation            | `enforce_external_only` pass               | None — `fti` is itself the signal |

Future merge with the reference: once the bootstrap grows enough
machinery to express the rest of the language, the `fti` keyword can
be re-spelled as `external fsm` and the inliner becomes a load-time
pass. The semantics survive.

---

End of spec.
