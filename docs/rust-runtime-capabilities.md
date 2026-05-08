# Evident — Rust Runtime Capabilities

**A reverse-engineered reference, derived purely from the Rust source.**

Generated 2026-05-08 from `runtime-rust/src/` (~14,500 LOC across 30 files).
File:line citations point into that tree. No spec files were consulted; if a
feature isn't here, the Rust runtime doesn't have it (or it isn't reachable
from any code path we found).

This document is intended as the canonical answer to: *if we threw out the
Python runtime tomorrow, what language would we have?*

---

## 0. Executive summary

The Rust runtime is a complete, self-contained Evident implementation with
a **bigger surface than the Python implementation in several dimensions**
(GLSL transpilation, SMT-LIB import/export, multi-format test harness,
multi-program executor) and **smaller in a few** (no batch/repl mode, no
named top-level enums, no inline anonymous enums, several composite-type
gaps in import/export).

Architecturally:

```
source text
  → lexer.rs                tokens (Unicode + ASCII operators, indentation)
  → parser.rs               AST (Program → SchemaDecl → BodyItem → Expr)
  → translate/preprocess    pinned ints, seq-length propagation, structural names
  → translate/declare       Z3 const declaration, sub-record expansion
  → translate/datatypes     Z3 algebraic datatypes for Seq(UserType) elements
  → translate/inline        claim composition: passthrough, mapsto, positional,
                            guarded, subclaim, names-match
  → translate/exprs         AST → Z3 (1295 lines: arithmetic, quantifier
                            unrolling, set/seq ops, record-lift)
  → translate/eval          solver lifecycle, model extraction, sampling
  → runtime.rs              EvidentRuntime API (load, query, sample, cache)
  → executor.rs             constraint-automaton step loop + multi-program
                            coordinator
  → plugins/                Stdin, Stdout, SDL, Audio, Shader
  → commands/               CLI surface (parse, query, test, execute, sample,
                            check, initial-state, transpile-shader,
                            export-smt2, import-smt2)
```

Two transpiler back-ends sit alongside Z3:

- **GLSL** (`glsl.rs` + `plugins/shader.rs`): topological-schedule constraint
  → fragment shader, with algebraic isolation for unknowns on the RHS.
- **SMT-LIB v2** (`smtlib.rs`): bidirectional. Export uses logic auto-selection
  for cross-solver portability (Z3, CVC5, Yices); import is partial round-trip.

What this runtime is *not*: there is no REPL, no batch-mode plugin, no named
top-level enum declarations, no inline anonymous enums, no user-defined
functions, no `let`-bindings in expressions, no native if-then-else (`ite`)
expression form, and no top-level `assert` for lookup tables (the parser does
not emit any `Assert` node — see §3.1).

---

## 1. Lexical structure

### 1.1 Tokens

The lexer (`lexer.rs`, 414 lines) is hand-rolled. It recognizes 50 token
kinds. Operators have **dual Unicode + ASCII spellings** that lex to the
same token — there is no separate normalizer pass.

| Operator         | Unicode  | ASCII      | Token         |
|------------------|----------|------------|---------------|
| Membership       | `∈`      | `in`       | `In`          |
| Non-membership   | `∉`      | —          | `NotIn`       |
| Reverse-in       | `∋`      | —          | `ContainsRev` |
| Equal            | `=`      | —          | `Eq`          |
| Not equal        | `≠`      | `!=`       | `Neq`         |
| Less / less-eq   | `<` `≤`  | `<` `<=`   | `Lt` `Le`     |
| Greater / ge     | `>` `≥`  | `>` `>=`   | `Gt` `Ge`     |
| And / Or / Not   | `∧` `∨` `¬` | —      | `And` `Or` `Not` |
| Implies          | `⇒`      | `=>`       | `Implies`     |
| Maps-to          | `↦`      | `mapsto`   | `MapsTo`      |
| For-all / Exists | `∀` `∃`  | —          | `ForAll` `Exists` |
| String concat    | —        | `++`       | `PlusPlus`    |
| Cardinality      | —        | `#`        | `Hash`        |

`!` standalone is a lex error (`unexpected '!'` at lexer.rs:329) — only `!=`
is recognized. There is **no `⟸` (reverse-implies) token** — the parser does
not handle it, despite CLAUDE.md mentioning it. `(A ⟸ B)` would be a parse
error.

Sequence-literal delimiters `⟨` (U+27E8) and `⟩` (U+27E9) are recognized.

Layout is significant: every line emits an `Indent(n)` token where `n` is the
column of the first non-whitespace character (tabs count as 4). Inside any
of `(`, `[`, `{`, `⟨`, newlines and indents are suppressed — multi-line
expressions inside groups are continuous.

### 1.2 Literals

- `Int(i64)` — decimal only (no hex, no underscores).
- `Real(f64)` — `<digits>.<digits>` with at least one digit on each side.
  The lexer disambiguates `3.foo` (Int + dot + ident) from `3.14` (Real) via
  one-char lookahead.
- `Str(String)` — double-quoted, single-line. Escapes: `\"`, `\\`, `\n`,
  `\t`. Any other `\x` is a lex error.
- `true` / `false` — boolean keywords. **Case-sensitive: lowercase only**.
  Capitalized `True`/`False` parses as bare identifiers and the resulting
  constraint fails to translate. CLAUDE.md describes this as a silent
  footgun; in the Rust runtime it's actually loud — you get
  `error: dropped constraint (couldn't translate to Bool): x = True` on
  the offending line, with an explanation.

### 1.3 Comments

Single-line only: `--` to end of line. No block comments, no doc comments.
Comment-only lines do not emit `Indent`.

### 1.4 Keywords

`schema`, `claim`, `type`, `subclaim`, `import`, `trace`, `send`, `key_down`,
`key_up`, `advance`, `shader`, `in`, `true`, `false`. That's it.

There is **no `assert` keyword** in the lexer or parser. CLAUDE.md mentions
`assert direction_exits = { … }` for lookup tables — that syntax is not
recognized by the Rust parser. Lookup tables would have to be expressed as
constraints inside a claim body (e.g., `("entrance", "north", "forest") ∈
exits_map ∧ …`).

There is also no `enum` keyword. **Named enums are not parseable** in the
Rust runtime (see §4.1).

---

## 2. Top-level declarations

The parser recognizes exactly six top-level forms (`parser.rs`):

| Form              | Notes                                                          |
|-------------------|----------------------------------------------------------------|
| `schema Name …`   | Equivalent to `claim`/`type` at runtime (different `Keyword` tag) |
| `claim Name …`    | Same                                                           |
| `type Name …`     | Same                                                           |
| `import "path"`   | String literal required; loaded eagerly at runtime             |
| `trace name "path" …` | Test trace block (see §9)                                  |
| `shader Name …`   | Fragment-shader declaration (see §13)                          |

**Everything else at file scope is a parse error.** No `assert`, no `enum`,
no top-level expressions or constraints.

### 2.1 Header forms

Schema headers accept an optional first-line parameter list:

```evident
type Vec2(x, y ∈ Int)               -- multi-name shorthand, single type
type Mixed(x, y ∈ Int, label ∈ Str) -- multiple typed groups
type Items(xs ∈ Seq(Int))           -- compound inner type
```

Compound first-line parameter types are limited to `Seq(T)`, `Set(T)`,
`Bag(T)`, and `Map(T)` (parser.rs `parse_first_line_params`). Pins
(e.g., `x ∈ Vec2(1, 2)`) are **not** allowed in first-line param positions —
only in body memberships.

### 2.2 Body items

Inside a schema body (`BodyItem`):

| Variant            | Syntax                                              |
|--------------------|-----------------------------------------------------|
| `Membership`       | `x ∈ T`, `x, y, z ∈ T`, `x ∈ T(pin1, pin2)` (positional), `x ∈ T(slot ↦ v)` (named) |
| `Passthrough`      | `..ClaimName`                                       |
| `SubclaimDecl`     | `subclaim Name …`                                   |
| `ClaimCall`        | `Name(slot ↦ value, …)` — recognized via lookahead  |
| `Constraint(Expr)` | Any boolean expression                              |

The parser uses lookahead to disambiguate: `IDENT ( IDENT MapsTo …` is a
named `ClaimCall`; everything else falls through to expression parsing,
which then becomes either an unwrapped identifier (a passthrough/names-match
invocation handled by the inliner) or a `Call(name, args)` (positional
invocation, recently added — see §6.3).

---

## 3. Operator precedence

### 3.1 Precedence table (lowest → highest)

```
1. Quantifiers      ∀ ∃                     right-associative, sucks up rest of line
2. Implies          ⇒  =>                   right-associative
3. Or               ∨                       left-associative
4. And              ∧                       left-associative
5. Comparison       = ≠ < ≤ > ≥ ∈ ∉ ∋       chained: `a ≤ b ≤ c` → `(a≤b) ∧ (b≤c)`
6. Additive         + - ++                  left-associative
7. Multiplicative   * /                     left-associative
8. Unary            ¬ - #                   prefix; `-x` desugars to `0 - x`
9. Atoms            literals, identifiers, ( ), calls, indexing, set/range/seq literals
```

This **matches** standard math conventions — and is the **opposite** of what
CLAUDE.md describes for `⇒` vs `∧`. CLAUDE.md says "⇒ binds tighter than ∧"
and warns that `A ⇒ B ∧ C` parses as `(A ⇒ B) ∧ C`. **The Rust parser does
the standard thing**: `A ⇒ B ∧ C` parses as `A ⇒ (B ∧ C)` because ⇒ is
above ∧. This is a CLAUDE.md inaccuracy, not a Rust runtime bug — but worth
flagging because the documented "footgun" doesn't fire here.

### 3.2 Chained comparison

`20 ≤ x ≤ 740` desugars in the parser to `(20 ≤ x) ∧ (x ≤ 740)`. The middle
operand is shared (math notation, not C left-fold). Works for any comparison
operators, not just inequalities.

### 3.3 Implies-block and quantifier-block forms

Both `⇒` and `∀ … :` accept indented blocks:

```evident
A ⇒
    B
    C       -- becomes A ⇒ (B ∧ C)

∀ i ∈ {0..3} :
    constraint1
    constraint2   -- becomes ∀ i ∈ {0..3} : (constraint1 ∧ constraint2)
```

The block is detected via `Newline` followed by deeper indent.

---

## 4. Type system

### 4.1 Built-in sorts

| Evident type   | Z3 representation                    | Notes                          |
|----------------|--------------------------------------|--------------------------------|
| `Int`          | `Int`                                |                                |
| `Nat`          | `Int` + asserted `>= 0`              | `declare.rs:89`                |
| `Pos`          | `Int` + asserted `> 0`               | `declare.rs:94`                |
| `Real`         | `Real`                               | extracted as `f64` (lossy)     |
| `Bool`         | `Bool`                               |                                |
| `String`       | `String`                             | Z3 native string sort          |
| `Seq(Int)`     | `Array(Int → Int)` + separate `len`  | not Z3 native `Seq`            |
| `Seq(Bool)`    | `Array(Int → Bool)` + separate `len` |                                |
| `Seq(String)`  | `Array(Int → String)` + separate `len` |                              |
| `Seq(Record)`  | `Array(Int → DatatypeSort)` + `len`  | `datatypes.rs:35`              |
| `Set(T)`       | Z3 `Set` (characteristic function)   | not enumerable in models       |

There are **no native enum types**. The CLAUDE.md examples like
`type Color = Red | Green | Blue` and `x ∈ Red | Green | Blue` will not
parse: there is no `=` in type-decl syntax and no `|` token type for
disjunction at the type level.

This means existing programs that use named enums (the spec mentions
`Verb`, `ItemKind`, etc.) will not load against the Rust runtime as written.
The workaround in current Rust-runtime programs is to use String constants.

### 4.2 Records

A `type` with field memberships becomes a "record" implicitly. There are
two representations depending on usage:

- **Direct membership** (`task ∈ Task` in a claim body) → no Z3 const for
  `task`; instead, one Z3 const per leaf field with a dotted name
  (`task.duration`, `task.deadline`, …). This is **sub-record expansion**.
- **Sequence element** (`tasks ∈ Seq(Task)`) → a Z3 algebraic datatype
  (`mk_Task` constructor + per-field accessors). Built lazily and cached
  in `DatatypeRegistry` (`types.rs:31`).

Nested records inside Datatypes are recursive — `Color { pos ∈ Point }`
inside a `Seq(Color)` builds a nested `DatatypeSort` for `Point`
(`datatypes.rs:78`).

**Field-of-Datatype-element access** (e.g., `state.dots[i].pos.x`) is
resolved by `resolve_seq_field` (`exprs.rs:91`), which walks the chain
outward-in, peeling nested datatypes and applying their accessors.

### 4.3 Type pins (defaults & partial instantiation)

```evident
v ∈ IVec2 (x ↦ 0, y ↦ 0)        -- named pins
p ∈ Vec2 (100, 200)              -- positional pins (declaration order)
c ∈ Color (30, 80)               -- partial positional (only first 2 fields)
```

Pins generate `name.field = value` constraints. Named is partial (omitted
slots stay free). Positional requires `args ≤ field count`.

### 4.4 Record literals as expressions

```evident
state.player.pos = IVec2(380, 280)
rect.color = Color(80, 200, 180)
```

Parsed as `Expr::Call("IVec2", [Int(380), Int(280)])`. Lifted via
`lift_record_op` componentwise across `=`, `≠`, `<`, `≤`, `>`, `≥` and
through `Binary` arithmetic during equation translation.

**Known gap**: `mapsto` does not resolve record-literal values.
`color ↦ Color(...)` silently drops the mapping (`resolve_mapping` doesn't
handle `Expr::Call`, `exprs.rs:28`). Workaround: bind to an intermediate
var first.

### 4.5 Vector / record lifting

When `lhs op rhs` involves record references and `op ∈ {=, ≠, <, ≤, >, ≥}`,
the operator broadcasts componentwise (`lift_record_op`, `exprs.rs:343`):

- `≠` folds with `Or` (some field differs).
- `=`, `<`, `≤`, `>`, `≥` fold with `And` (every field).

Detected record-reference shapes:

1. `Identifier("name")` where env contains `name.*` keys.
2. `Field(Index(Identifier(seq), idx), field)` (record-element field of seq).
3. `Index(Identifier(seq), idx)` (whole record element of seq).
4. `Call(type_name, args)` (record literal).

Arithmetic broadcast (`c = a - b`, `nxt.pos = cur.pos + cur.vel * dt`) works
through the same machinery. Shape mismatches (Vec2 = Vec3) fail via the
"dropped constraint" policy.

### 4.6 Things that don't exist

- **Named enum declarations** — no syntax, no AST node.
- **Inline anonymous enums** (`x ∈ Red | Green | Blue`) — no `|` operator
  in the parser.
- **`Seq(T)` or `Set(T)` as record fields** — `datatypes.rs:94` rejects with
  "unsupported field type" when building the Datatype.
- **`Bag(T)`, `Map(T)`** — recognized as syntactic compound types in headers
  but no translator support.
- **`let`-bindings** in expressions.
- **`if-then-else` (`ite`)** as a value-producing expression. Use
  `(c ⇒ a) ∧ (¬c ⇒ b)` style instead (the SMT-LIB importer auto-encodes
  `ite` this way on import).

---

## 5. Expressions

### 5.1 AST node variants

```rust
enum Expr {
    Identifier(String),       // bare or dotted (greedy at parse time)
    Int(i64), Real(f64), Bool(bool), Str(String),
    SetLit(Vec<Expr>),        // {a, b, c} — only valid as RHS of ∈
    SeqLit(Vec<Expr>),        // ⟨a, b, c⟩
    Range(Box<Expr>, Box<Expr>), // {lo..hi} — only valid as quantifier bound
    InExpr(Box<Expr>, Box<Expr>), // lhs ∈ rhs
    Forall(Vec<String>, Box<Expr>, Box<Expr>),
    Exists(Vec<String>, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),  // name(arg, …) — function or record literal
    Cardinality(Box<Expr>),   // #expr
    Index(Box<Expr>, Box<Expr>),
    Field(Box<Expr>, String),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
}

enum BinOp {
    Eq, Neq, Lt, Le, Gt, Ge,
    And, Or, Implies,
    Add, Sub, Mul, Div,
    Concat,                   // ++ string concatenation
}
```

### 5.2 Built-in functions

Recognized by name in `Call(name, args)`:

- `coindexed(seq1, seq2, …)` — n-ary parallel zip; only valid in quantifier
  range position.
- `edges(seq)` — adjacent-pair iterator; only valid in quantifier range
  position.

Plus type-name-as-record-literal calls (any user-defined type).

### 5.3 Translation paths

`translate_int`, `translate_bool`, `translate_str`, `translate_real`
(in `exprs.rs`). Each returns `Option<ZType>`; `None` triggers the
"dropped constraint" warning at the inliner level (`inline.rs:336`).

Coercion: `Int → Real` happens automatically inside `translate_real`
(`exprs.rs:274`); other coercions are not done.

---

## 6. Quantifiers, iteration helpers, and unrolling

### 6.1 Quantifier shapes

```evident
∀ i ∈ {0..n-1} : body              -- single var, integer range
∀ v ∈ seq : body                    -- single var, seq element
∀ (a, b) ∈ coindexed(s1, s2) : body -- tuple binding via coindexed
∀ (a, b) ∈ edges(seq) : body        -- adjacent-pair binding via edges
∃ i ∈ {0..n} : body                 -- same forms with ∃
```

Tuple binding requires ≥ 2 names (parser enforces). Single-name binding to
a `coindexed`/`edges` is not valid.

### 6.2 Unrolling, not symbolic quantification

The Rust runtime **unrolls all quantifiers at translation time** rather
than emitting Z3 `forall`/`exists` quantifiers (`exprs.rs:1100`). This
means:

- All bounds must reduce to concrete `(lo, hi)` integers via
  `literal_range`. Symbolic bounds → silent no-op (zero iterations,
  no warning logged).
- Sequence lengths must be **pinned** (see §6.3) for `∀ v ∈ seq` and
  `coindexed`/`edges` to unroll.
- Quantifier bodies are emitted as `And` of N copies for ∀, `Or` of N
  copies for ∃.

This is much more amenable to Z3's solver than symbolic quantifiers, but
it means programs with unbounded universal quantification will silently
do nothing.

### 6.3 Seq-length pinning

`preprocess.rs:238` (`collect_seq_lengths`) iterates to fixed point,
pinning `len` from three sources:

1. Given `Seq` values (length from `Vec.len()`).
2. Sequence-literal constraints: `seq = ⟨e1, …, en⟩` pins `#seq` to `n`.
3. Cardinality equalities: `#seq = expr` where `expr` reduces to a literal
   under already-pinned vars. Iterates until no progress.

**Caveats:**

- Pinning works only on top-level body items. Inner-claim Memberships
  whose lengths are pinned only via cross-claim parameters won't propagate
  (CLAUDE.md mentions this; confirmed in `preprocess.rs`).
- Symbolic length pinning (`#s = some_int_var`) doesn't work — the var
  must reduce to a concrete literal via the pinned-int pass.

### 6.4 Pinned ints

Parallel to seq-lengths, `collect_pinned_ints` (`preprocess.rs:158`) folds
`name = literal_expr` constraints into `Var::PinnedInt(value)`. Pinned ints
substitute as Z3 literal `IntVal` in expression translation (`exprs.rs:204`),
which is what makes `∀ i ∈ {0..n-1}` unroll when `n = 8` is asserted.

---

## 7. Composition: every form of claim invocation

The inliner (`translate/inline.rs`, 407 lines) is where the language's
composition mechanics live. Six forms are handled:

### 7.1 Bare-name names-match

```evident
claim my_problem
    schedule ∈ Set Assignment
    rooms_conflict_free          -- bare name, names-match composition
```

Detected as `Constraint(Identifier(name))` where `schemas.contains_key(name)`
(`inline.rs:220`). The claim's body is inlined into the parent env as-is;
variables with matching names resolve to the same Z3 const.

### 7.2 `..ClaimName` passthrough

```evident
type main
    ..LineReader
    ..LineWriter
```

Detected as `Passthrough(name)` (`inline.rs:349`). Identical semantics to
bare-name; the syntactic form is a stylistic choice for "I'm including
this module's fields directly into my scope."

### 7.3 Named mapsto invocation

```evident
manage_event:
    Conference.valid (schedule ↦ assignments)
```

Detected via lookahead in the parser as `ClaimCall { name, mappings }`
(`inline.rs:361`). For each mapping, `resolve_mapping` (`exprs.rs:28`) tries:

1. Exact env-key match (scalar substitution).
2. Sub-schema expansion (if the slot is a record type and the value's
   `name.*` keys exist in env).
3. `expr_as_var` — bare identifier or literal.

Unmapped slots get fresh Z3 consts with per-call ID mangling
(`ClaimName__slot__call<id>`).

### 7.4 Positional invocation (newest)

```evident
claim Distinct(s ∈ Seq, n ∈ Nat)
    ∀ i ∈ {0..n-1} : ∀ j ∈ {0..n-1} : i < j ⇒ s[i] ≠ s[j]

claim my_problem
    items ∈ Seq(Int) ; #items = 8
    Distinct(items, 8)              -- positional; no mapsto
```

Detected as `Constraint(Call(name, args))` where `schemas.contains_key(name)`
(`inline.rs:244`, added in commit `4e39dd8`). Pairs args with the claim's
first N `Membership` body items in order, converts to `Mapping` structs,
then proceeds identically to §7.3.

This is the recommended style for stdlib-like callable claims (CLAUDE.md
"Interface vars on the claim line + positional invocation" section).

### 7.5 Guarded invocation

```evident
state.step = 0 ⇒ InitGameState
```

Detected as `Constraint(Binary(Implies, ant, Identifier(name)))` where the
identifier names a known claim (`inline.rs:309`). Compose with any outer
guard via `compose_guards` (`outer ∧ ant`). Constraints inside the inlined
body get wrapped in `guard ⇒ …`; **declarations are unconditional** (the
fresh Z3 consts always exist).

### 7.6 Subclaim definitions

```evident
claim GameTransition
    state ∈ GameState
    subclaim LookAction
        state_next.location = state.location
```

Stored as `BodyItem::SubclaimDecl(SchemaDecl)`. Lifted into the runtime's
schemas table at `load_source` time so other claims can reference them
(`runtime.rs`). The subclaim inherits the parent's variables by names-match
via the visited-set / cloned-env mechanism. Internal vars in the subclaim
body are fresh — not visible to the parent.

### 7.7 Recursion guard

The `visited: HashSet<String>` accumulates claim names currently being
inlined (`inline.rs:44`). Prevents infinite recursion from cyclic
passthroughs.

---

## 8. The runtime API (`runtime.rs`, 571 lines)

`EvidentRuntime` is the main entry point. Surface:

| Method                   | Purpose                                                  |
|--------------------------|----------------------------------------------------------|
| `new()`                  | Fresh runtime; leaks a Z3 `Context` (one per process)    |
| `load_source(src)`       | Parse + load source string                               |
| `load_file(path)`        | Load from disk; tracks canonical path for import resolution |
| `query(name, given)`     | One-shot SAT/UNSAT decision                              |
| `query_cached(name, given)` | Cached solver with structural-signature invalidation  |
| `query_with_core(name, given)` | Like query, also returns UNSAT core indices         |
| `query_free(name)`       | `query` with empty givens                                |
| `sample(name, given, n)` | Up to n distinct models via blocking-clause loop         |
| `schema_names()`         | Iterator over all loaded schemas (incl. lifted subclaims) |
| `get_schema(name)`       | Lookup                                                   |
| `traces()`               | Slice of parsed `TraceDecl`                              |
| `shaders()`              | Slice of parsed `ShaderDecl`                             |
| `cache_rebuilds()`       | Counter for structural-signature mismatches              |

### 8.1 Import resolution

`load_file` resolves `import "..."` paths in this order (`runtime.rs:389`):

1. Verbatim (absolute or cwd-relative).
2. Relative to the importing file's directory.
3. Cwd-relative (explicit fallback).
4. Project-root-relative — walks upward up to 10 levels from the source
   file's directory looking for `programs/`.

Embedded **stdlib shims** are auto-loaded if no real file exists on disk:
`stdlib/sdl.ev`, `stdlib/io.ev`. This is how programs that declare
`∈ SDLOutput` work without an explicit import.

### 8.2 Cached query / structural signature

`query_cached` keeps the Z3 solver alive across calls with `push`/`pop`
per query. It detects which `given` keys appear in **quantifier bounds**
(the "structural signature"). If the signature changes, the cache is
rebuilt; otherwise just the values are re-asserted. This is what makes
the per-frame executor performant: per-frame state changes (player
position, frame counter) don't trigger rebuild; config changes (level
size) do.

### 8.3 Sampling

`sample(name, given, n)` runs a blocking-clause loop:

1. `solver.push()`, assert givens.
2. Check SAT, extract bindings.
3. Assert `¬(b1 = v1 ∧ … ∧ bn = vn)` to block this model.
4. Repeat until UNSAT or n models.

**Limitation**: only Bool/Int/Str bindings contribute to the blocking
clause (`runtime.rs:539`). Schemas with only Seq/Set outputs may return
duplicates. Sample queries also use a fresh "safe" solver
(`arith.solver=0`) to avoid pathological slowness as the blocking clause
grows.

---

## 9. CLI (`commands/`)

The binary is `evident <subcommand>`. All subcommands:

| Subcommand           | Purpose                                                  |
|----------------------|----------------------------------------------------------|
| `parse <file>`       | Debug: parse + print schema names. Exit 1 on parse error. |
| `query <files…> <schema> [--given k=v …] [--json] [--explain]` | Single SAT/UNSAT decision |
| `check <files…>`     | Query every loaded schema with empty givens; SAT/UNSAT report |
| `sample <files…> <schema> [-n N] [--given k=v] [--json]` | Up to N distinct models |
| `test [path] [-v] [--no-color] [--format=tap\|junit\|json]` | Test discovery + run |
| `execute <file> [SDL flags] [--initial-state PATH]` | Run `schema main` as constraint automaton |
| `transpile-shader <file> <shader_name>` | Emit GLSL                                |
| `export-smt2 <file> <claim>` | Emit SMT-LIB v2                                  |
| `import-smt2 <file> [claim_name]` | Parse SMT-LIB v2 → Evident                  |
| `initial-state <file> <claim>` | Generate initial-state JSON for executor seeding |

Conspicuously absent vs. Python: **no `batch` mode, no `repl`**. These
were "parked behind plugin work" per the executor comments; users would
need to fall back to Python for those.

---

## 10. Test harness (`commands/test.rs`, 914 lines)

`evident test [path]` discovers test files matching `test_*.ev` and runs:

### 10.1 Test conventions

- **Schema tests**: claims named `sat_*` (expect satisfiable) or
  `unsat_*` (expect unsatisfiable).
- **Trace tests**: top-level `trace name "path/to/program.ev"` blocks
  containing `send`/`key_down`/`key_up`/`advance` steps with inline
  assertions (`output = "expected"`, `var ∋ "substring"`).

### 10.2 Output formats

- **Default** — dots + summary; FAILURES section with constraint-highlighted
  bodies and contextual bindings.
- `-v` / `--verbose` — per-test PASS/FAIL/ERROR lines with elapsed time.
- `--format=tap` — TAP v14 with YAML diagnostic blocks.
- `--format=junit` — JUnit XML, testsuite-per-file grouping.
- `--format=json` — `{ summary: { passed, failed, errors, elapsed_ms },
  results: [...] }`.

Color is auto-on for TTY (detected via `isatty(stdout)` FFI), suppressed
by `--no-color` or `NO_COLOR` env var. Exit code 1 on any failure.

UNSAT-core highlighting: when a `sat_*` test fails, the harness uses
`query_with_core` to point at the specific body items in conflict.

---

## 11. Executor: the constraint automaton

`executor.rs` (1118 lines) implements the per-step solve loop for
`evident execute <file>`. Required shape: the entry schema is named
**`main`** (claim or type), and contains:

- I/O port memberships (`∈ Stdin`, `∈ Stdout`, `∈ SDLInput`, etc.)
  matched against plugin `handles_types`.
- One or more state pairs: `state ∈ T` and `state_next ∈ T` for the same
  non-I/O type (auto-detected by name + type).

### 11.1 Step loop

1. Each plugin's `before_step` is called → contributes givens
   (`var.char`, `var.left_held`, `input.dt`, …).
2. Current state is loaded as givens: every `state.field` value from the
   prior step.
3. `query_cached("main", given)` runs.
4. **SAT**: `state_next.*` bindings become next step's `state.*`. Plugins'
   `after_step(bindings)` runs side effects (stdout writes, SDL render).
5. **UNSAT**: state preserved (no transition), warning printed unless
   `--quiet`. `--explain` dumps the full body + givens.
6. If any plugin's `before_step` returns `None` or `after_step` returns
   `false`, the executor halts.

### 11.2 Type defaults for first-frame state

Initial state is synthesized from type defaults: `Nat`/`Int` → 0, `Bool`
→ false, `String` → "", `Seq` → empty. Override via `--initial-state PATH`
(JSON file with top-level object).

### 11.3 Multi-program coordinator

`run_with_main_coordinator` (`executor.rs:620`) supports menu/level
transitions:

- Programs declare `..MainCoordinator` (or it's auto-embedded) and emit
  `next_main = "<path>"` or `next_main = "halt"` per step.
- Executor reads this field, swaps program file, re-loads runtime.
- A single `world` / `world_next` state pair survives the swap; other
  state resets.
- Plugins activate once on first program; later programs must use the
  same SDL/audio var names.
- LRU cache of 8 programs kept warm to avoid reload cost on menu
  back-and-forth.

### 11.4 Plugin architecture

```rust
trait Plugin {
    fn handles_types(&self) -> &'static [&'static str];
    fn initialize(&mut self, matched_vars: Vec<(String, String)>);
    fn before_step(&mut self) -> Option<HashMap<String, Value>>;
    fn after_step(&mut self, bindings: &HashMap<String, Value>) -> bool { true }
}
```

Built-in plugins:

| Plugin                | `handles_types`                          | What it does          |
|-----------------------|------------------------------------------|-----------------------|
| `StdinPlugin`         | `Stdin`, `CharInput`                     | one byte/step + EOF   |
| `StdoutPlugin`        | `Stdout`, `Stderr`, `CharOutput`         | writes `var.out`      |
| `SDLPlugin`           | `SDLInput`, `SDLOutput`, `SDLWindow`     | window + render rects |
| `SDLAudioPlugin`      | `SDLAudio`                               | sine/square synth     |
| `SDLShaderPlugin`     | `SDLShaderOutput`, `SDLInput`, `SDLWindow` | compiles + runs shader |

Each plugin auto-activates only if at least one matching type is declared
in `main`. Plugins not matching `main`'s vars are zero-cost (skipped).

Embedded stdlibs ship with each plugin — programs declaring `∈ SDLInput`
don't need an explicit type definition; the plugin contributes `IVec2`,
`Color`, `SDLRect`, `SDLInput`, `SDLOutput`, `SDLWindow` definitions.

### 11.5 Footguns

- **Blocking I/O conflict**: declaring both `∈ Stdin` and `∈ SDLInput`
  causes stdin's blocking read to freeze the SDL event loop. No automatic
  detection.
- **VSync coupling**: SDL renders with vsync on by default
  (`gl_set_swap_interval(SwapInterval::VSync)`, `shader.rs:162`). Disable
  with `EVIDENT_SDL_NO_VSYNC=1` for performance benchmarking.

---

## 12. Trace runner (`trace_runner.rs`, 533 lines)

Executes `trace name "path"` blocks step-by-step for the test harness.
Supports two execution modes via the same step loop:

- **Stdin mode** (`src ∈ Stdin`): `send "cmd"` feeds chars + newline
  one-by-one, breaks when `line_ready=true`.
- **SDL mode** (`input ∈ SDLInput`): `key_down`/`key_up` toggle held-key
  state, `advance T` ticks 16ms per frame for T ms, contributes
  `input.<key>_held` per frame.

Frame constants in trace mode: 16 ms dt (60 Hz), 800×600 fixed window,
mouse stuck at (0, 0), click/quit always false. Held-key state persists
across `advance` steps.

Trace assertion targets:

- `output` (literal name) → accumulated stdout text.
- Any other identifier → state field (flat-mapped; first match wins).

Operators: `=` (equality) or `∋` (substring containment).

---

## 13. GLSL transpilation

### 13.1 Pipeline

`evident transpile-shader <file> <shader_name>` produces a GLSL fragment
shader. The runtime's `SDLShaderPlugin` does the same transpile + compile +
upload at execution time when a program declares `∈ SDLShaderOutput`.

The `ShaderDecl`'s body must contain only `Membership` and `Constraint`
items (`glsl.rs:199`). Passthroughs, claim calls, and subclaims are
explicitly rejected.

### 13.2 Required vars

- `pixel ∈ Vec2` — fragment coordinate in normalized [0, 1] space, supplied
  by the vertex shader template.
- `output.fragment ∈ Color` (or `∈ Vec4`) — final RGBA output.

Built-in uniform: `iResolution.x`, `iResolution.y` — auto-injected per
frame.

### 13.3 Type mapping

| Evident   | GLSL                  |
|-----------|------------------------|
| `Real`    | `float`                |
| `Int`/`Nat`/`Pos` | `int`          |
| `Bool`    | `bool`                 |
| `Vec2`/`Vec3`/`Vec4` | `vec2`/`vec3`/`vec4` |
| `Color`   | `vec3` (always shader-local in v1; not a uniform) |
| User record (e.g., `state ∈ GameState`) | flattened to per-leaf scalar uniforms with `_`-joined names |

### 13.4 Wave scheduling + algebraic isolation

The transpiler doesn't emit constraints in source order — it does
**wave scheduling** (`glsl.rs:288`). Each pass: find a constraint with
exactly one undefined variable among its references, emit it, mark that
variable defined. Repeat. If stuck, report underdetermined (multiple
unknowns) or cyclic (a depends on b depends on a).

When the unknown is on the RHS (`a + b = c` solving for `a`), the
transpiler **algebraically isolates** the unknown via `isolate`
(`glsl.rs:496`). Supports `+`, `-`, `*`, `/` chains. Rejects:

- Function calls on the unknown side (`length(c) = d` for `c`).
- Multi-occurrence variables (`a + a = c`, `a * a = c`) — would need
  quadratic-formula reasoning.

### 13.5 Guarded constraints

`cond ⇒ var = expr` becomes a GLSL `if` statement. The LHS must still be
a bare identifier; `cond` and the RHS must be fully resolved (no undefined
locals).

### 13.6 Sub-record synthesis

`state.hero` (where leaves are `state_hero_x`, `state_hero_y` uniforms)
becomes `vec2(state_hero_x, state_hero_y)` (`glsl.rs:774`). One level of
nesting only.

### 13.7 Footguns

- **Quantifiers, sets, sequences are hard-rejected** in shader bodies
  (`glsl.rs:598`). Loop-style work has to be hand-unrolled in the source.
- **Swizzles on synthesized records** (e.g., `state.hero.xy` if `state.hero`
  is synthesized) are not supported.
- **Non-swizzle field access on unknown leaves** (`a.foo` where `foo` isn't
  a vector component) errors with "non-swizzle field" message.
- **Color uniforms** don't exist in v1 — `Color` is always shader-local.

### 13.8 Plugin runtime (`plugins/shader.rs`, 443 lines)

Shader plugin lifecycle:

1. `initialize` — creates SDL window + GL 3.3 core context. Idempotent
   (re-init reuses).
2. First post-solve `before_step` — reads `output.shader_name` binding,
   transpiles + compiles + links, caches program + uniform locations.
3. Per `after_step` — pulls each uniform's value from bindings, uploads
   via `glUniform*`, draws fullscreen quad, swaps buffers.

Optimized-away uniforms (location -1) silently no-op rather than erroring.

---

## 14. SMT-LIB v2 import/export (`smtlib.rs`, 932 lines)

### 14.1 Export

`evident export-smt2 <file> <claim>` emits:

```
; Generated by Evident — claim: <name>
(set-logic <auto-selected>)
(declare-const <var> <sort>)         ; one per leaf
…
(assert <constraint>)                 ; one per body item, plus pin constraints
…
(check-sat)
(get-model)
```

Type mapping:

| Evident   | SMT-LIB                                          |
|-----------|--------------------------------------------------|
| `Int`     | `Int`                                            |
| `Nat`     | `Int` + `(assert (>= var 0))`                    |
| `Pos`     | `Int` + `(assert (> var 0))`                     |
| `Real`    | `Real`                                           |
| `Bool`    | `Bool`                                           |
| `String`  | `String` (Z3 + CVC5 only)                        |

Operators map straightforwardly: `+ - *` → same; `/` → `div`; `=` → `=`;
`≠` → `distinct`; `∧ ∨ ⇒ ¬` → `and or => not`; `++` → `str.++`.

Sub-records flatten: `task ∈ Task` → `(declare-const task.field …)` per
leaf, plus Task's own constraints rewritten under the `task.` prefix.

### 14.2 Logic auto-selection

The exporter walks the schema (incl. sub-records) to classify features
(`smtlib.rs:95`):

| Features used                                | Logic emitted |
|----------------------------------------------|---------------|
| `String` present                             | `ALL`         |
| `Real` + quantifiers                         | `LRA`         |
| `Real` no quantifiers                        | `QF_LIRA`     |
| Quantifiers no `Real`                        | `LIA`         |
| Otherwise                                    | `ALL`         |

`LIA` is the sweet spot — accepted by both Z3 and Yices2. Strings are
fragile (only Z3 + CVC5).

### 14.3 Pins

- **Named pins**: `task ∈ Task(duration ↦ 5)` → `(assert (= task.duration 5))`.
- **Positional pins**: **not yet supported in export** (`smtlib.rs:307`).
  Errors with "positional pins on `name` not yet supported; use named-pin
  form."

### 14.4 Import

`evident import-smt2 <file> [claim_name]` parses an SMT-LIB v2 script and
produces a single Evident claim. Recognized forms:

- `(declare-const name sort)` → `Membership`.
- `(declare-fun name () sort)` → same (zero-arg function).
- `(declare-fun name (args…) sort)` (higher-arity) → **error**.
- `(assert expr)` → `Constraint`.
- Solver directives (`set-option`, `set-logic`, `check-sat`, `get-model`,
  `exit`) → silently ignored.

Sort translation: `Int`, `Real`, `Bool`, `String` map directly. Anything
compound (`(_ BitVec 32)`, `(Array …)`) → error.

### 14.5 Bounded quantifier round-trip

The importer reverses the exporter's encoding:

- `(forall ((x Int)) (=> (and (>= x lo) (<= x hi)) body))` → `Forall([x], Range(lo, hi), body)`.
- `(exists ((x Int)) (and (>= x lo) (<= x hi) body))` → `Exists([x], Range(lo, hi), body)`.

Limitations:

- **Single binder only**. `(forall ((x Int) (y Int)) …)` errors.
- **Bounded only**. Unbounded quantifiers and non-Int ranges error.
- **`ite` (if-then-else)** is encoded as `(cond ⇒ a) ∧ (¬cond ⇒ b)` —
  semantically valid; awkward for non-Bool results.

### 14.6 Out-of-scope on import

Sets, set membership, set literals, sequence literals, sequence indexing,
record field access at top level, function calls — all error with
"not in scope (in v1)" messages.

---

## 15. Solver tuning & diagnostics

### 15.1 Auto-tuner (`runtime.rs:98`)

On first use of `query_cached`, the runtime measures Z3's `smt.arith.solver`
setting. Candidates: 2 (older Simplex; wins on Z3 4.8.x) and 6 (newer
default; wins on newer Z3). 30 frames per candidate, then locks to fastest.

### 15.2 Environment variables

| Env var                          | Effect                                              |
|----------------------------------|-----------------------------------------------------|
| `EVIDENT_Z3_AUTOTUNE=0`          | Disable pricing; lock to `EVIDENT_Z3_ARITH_SOLVER` |
| `EVIDENT_Z3_AUTOTUNE_LOG=1`      | Log pricing decisions to stderr                    |
| `EVIDENT_Z3_ARITH_SOLVER=N`      | Override (default 2; one-shot queries always use this) |
| `EVIDENT_BENCH=1`                | Per-second timing breakdown in executor            |
| `EVIDENT_SDL_FPS=1`              | FPS overlay + per-second reporter in SDL           |
| `EVIDENT_SDL_NO_VSYNC=1`         | Disable VSync (for solver perf measurement)        |
| `EVIDENT_LENIENT=1`              | Pin failures warn instead of erroring              |
| `NO_COLOR`                       | Suppress color in test output                      |

---

## 16. Known limitations and partial implementations

Catalogued from in-source TODOs, FIXMEs, and "not yet supported" branches:

### 16.1 Type system

- **No named enum declarations** — no syntax in parser; `enum` is not a
  keyword; `|` is not a type-disjunction operator. Programs using `Verb`,
  `ItemKind`, etc. will not load.
- **No inline anonymous enums** — same reason.
- **No `assert <name> = { … }` lookup-table syntax** — must inline as
  set-membership constraints.
- **No `let`-bindings, no `ite` value form**.
- **No `Seq(T)` or `Set(T)` as record fields** (`datatypes.rs:94`).
- **Real values lossy** — extracted as `f64` (`types.rs:54`).

### 16.2 Translator

- **Quantifier unrolling silent on symbolic bounds** — `∀ i ∈ {0..n}` with
  unpinned `n` produces zero iterations and no warning (`exprs.rs:1100`).
- **Mapsto doesn't resolve record literals** — `color ↦ Color(...)` silently
  drops (`exprs.rs:28`). Use intermediate variable.
- **Constraint dropping on translation failure** — warns at `inline.rs:336`,
  but quantifier-unrolling-failure and field-resolution-failure are silent.
- **Set enumeration not supported** in model extraction (`eval.rs:343`).

### 16.3 Runtime / executor

- **Sample blocking-clause** only blocks Bool/Int/Str — Seq/Set may dup.
- **Multi-program plugin re-activation** — plugins activate on first
  program only; later programs must reuse var names.
- **Reserved unused flags** — `--host`, `--port` parsed but not used.
- **`Composite` and `SeqComposite` value formatting** falls back to Debug
  format (`commands/common.rs:156`).
- **No batch mode, no REPL** — would require Python fallback.

### 16.4 SMT-LIB

- **Positional pin export not implemented** (`smtlib.rs:307`).
- **Single-binder quantifier import only** (`smtlib.rs:809`).
- **Bounded ranges only on import**.
- **`ite` import** uses verbose `(c ⇒ a) ∧ (¬c ⇒ b)` encoding.
- **Strings only round-trip Z3/CVC5** — Yices rejects.
- **No round-trip preservation** of subclaim composition or comments.

### 16.5 GLSL

- **No quantifiers, no sets, no sequences** in shader bodies.
- **No multi-occurrence variables** in algebraic isolation.
- **Function calls on the unknown side** of an equation can't be inverted.
- **Color uniforms** don't exist (only shader-local Color).
- **Custom vertex shaders** not supported (template hardcoded).
- **One level of sub-record synthesis** only.

### 16.6 Parser quirks worth knowing

- **Capitalized `True`/`False`** parses as bare identifiers, not booleans.
  Unlike the silent CLAUDE.md footgun, the Rust translator catches this
  loudly: `error: dropped constraint (couldn't translate to Bool): x = True`.
- **`⟸` (reverse-implies)** is not a recognized token. The dispatch-table
  syntax `Action ⟸ verb = Go` from CLAUDE.md does not parse.
- **Greedy dotted identifiers** — `state.dots[i]` parses as
  `Identifier("state.dots")` then `Index`. Post-index `.x` becomes a
  separate `Field` node. Most code doesn't notice, but it matters for
  the record-lift heuristics in §4.5.

---

## 17. File index

For verifying any claim above, here's where to look in `runtime-rust/src/`:

| Topic                            | File                                  |
|----------------------------------|---------------------------------------|
| Tokens                           | `lexer.rs`                            |
| Grammar / AST construction       | `parser.rs`                           |
| AST node definitions             | `ast.rs`                              |
| Z3 sort registry, datatypes      | `translate/types.rs`, `translate/datatypes.rs` |
| Z3 const declaration             | `translate/declare.rs`                |
| Pinned-int + seq-length passes   | `translate/preprocess.rs`             |
| Expression → Z3                  | `translate/exprs.rs`                  |
| Solver lifecycle, sampling       | `translate/eval.rs`                   |
| Claim composition (all 6 forms)  | `translate/inline.rs`                 |
| Model value formatting           | `translate/extract.rs`                |
| Top-level Runtime API            | `runtime.rs`                          |
| Step loop, multi-program         | `executor.rs`                         |
| Test trace replay                | `trace_runner.rs`                     |
| CLI dispatcher                   | `main.rs`, `commands.rs`              |
| Test harness (914 lines!)        | `commands/test.rs`                    |
| Execute (constraint automaton)   | `commands/execute.rs`                 |
| GLSL transpiler                  | `glsl.rs`                             |
| Shader plugin runtime            | `plugins/shader.rs`                   |
| SDL + Audio plugins              | `plugins/sdl.rs`, `plugins/audio.rs`  |
| SMT-LIB v2 export / import       | `smtlib.rs`                           |

---

## 18. Migration assessment summary

If switching from Python to Rust *today*, the things that would
**immediately break** existing programs:

1. **Programs using named enums** (`type Color = Red | Green | Blue` and
   `x ∈ Red | Green | Blue`). The Rust runtime has no enum syntax. Workaround:
   String constants.
2. **Programs using `assert <name> = { … }` lookup tables.** The Rust
   parser doesn't recognize `assert`. Workaround: express as set-membership
   constraints inside a claim body.
3. **Programs using `⟸` (reverse-implies) for dispatch tables.** No
   Rust token. Workaround: write as `B ⇒ A`.
4. **Programs that depend on quantifier symbolic bounds** without seq-length
   pinning. Silent zero-iteration unroll.
5. **Programs that pass record literals via `mapsto`** — silently drop.
   Workaround: intermediate variables.

Things that **work better** in Rust:

- Multi-format test harness (TAP, JUnit, JSON).
- Multi-program coordinator for menu/level transitions.
- GLSL shader transpilation with topological scheduling.
- SMT-LIB import/export with logic auto-selection.
- Solver auto-tuning + structural-signature query caching.
- Loud-by-default UNSAT reporting in the executor.

Things **not in Rust**: REPL, batch mode, anything driven by `evident.py`'s
top-level commands not listed in §9.
