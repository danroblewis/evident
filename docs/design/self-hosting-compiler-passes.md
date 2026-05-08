# Self-Hosting Compiler Passes as Evident Programs

## The Core Insight

Evident is a constraint-solving language whose runtime already does one
thing well: take a constraint model, hand it to Z3, return a satisfying
assignment.

Most compiler infrastructure — type inference, static checks, syntactic
desugaring, certain optimizations — is *itself* constraint-based
reasoning. The conventional approach builds these passes as procedural
code in the runtime's host language (Rust, Python, etc.) and pays the
cost N times for N runtime implementations.

The alternative: write each pass as an Evident program over a canonical
AST representation. Any runtime that implements the core
(parse → AST → Z3 query) inherits every pass for free by loading and
running the corresponding `stdlib/*.ev` file.

This is the self-hosting compiler pattern, applied to a constraint
language. It's unusually clean for Evident because the runtime isn't
being asked to evaluate procedural code — it's being asked to do
exactly what it already does.

---

## What This Buys

**For runtime authors:** the surface to implement shrinks from
~14,000 LOC (the current Rust runtime) to maybe ~5,000 — lexer,
AST builder, Z3 bridge, and a small executor loop. Everything else
ships as `.ev` files.

**For language evolution:** new compiler passes become PRs to a
shared stdlib instead of N parallel implementations across runtimes.
A new chained-membership desugar, a new linter rule, a new
optimization — all written once and immediately available everywhere.

**For inspectability:** the rules are written in the same language
as user code. Reading "how does type inference work?" means reading
an `.ev` file with named claims, not following a procedural codebase.
Users can extend or override individual rules in their own programs.

**For correctness:** the same solver that runs user programs also
type-checks them. Bugs in inference logic surface as UNSAT or
multiple-model results — debuggable with the same tools (`--explain`,
`evident sample`, etc.).

---

## What's Self-Hostable

Components that fit naturally into "input AST + rules → output":

### 1. Type inference (the headline use case)

Input: a parsed program. Output: a type assignment per variable, or a
list of conflicts.

The inference rules are constraints over a `TypeVar` enum:

```evident
enum TypeKind = Int | Nat | Pos | Real | Bool | String | SeqOf TypeKind | …

claim TypeFromLiteral
    expr ∈ Expr
    var  ∈ Identifier
    type ∈ TypeKind
    expr = Binary(Eq, var, IntLit) ⇒ type ∈ {Int, Nat, Pos, Real}
    expr = Binary(Eq, var, BoolLit) ⇒ type = Bool
    expr = Binary(Eq, var, StrLit) ⇒ type = String

claim ConsistentTyping
    program ∈ Program
    types ∈ Seq(TypeAssignment)
    ∀ c ∈ program.body : ApplicableRule(c, types)
```

Z3 finds an assignment satisfying every rule. Multiple models =
ambiguity; UNSAT = type conflict.

### 2. Static analyses

- **Lookup-table completeness**: assert that every key in the table's
  domain appears in the table.
- **Unused variable detection**: a variable is unused iff it doesn't
  appear in any constraint.
- **Name shadowing**: detect when an inner-claim variable name collides
  with an outer-scope name.
- **Cycle detection in passthrough composition**: detect `..A` chains
  that loop.

Each is a constraint over the AST that's either SAT (valid) or UNSAT
(reportable issue). The Evident runtime's existing UNSAT-core
machinery makes the error reporting almost automatic.

### 3. Syntactic desugaring

Patterns that today live in the parser as procedural code:

- **Chained membership** (`0 < x ∈ Int < 5` → `x ∈ Int ∧ 0 < x ∧ x < 5`)
- **Multi-name shorthand** (`x, y, z ∈ Int` → three Memberships)
- **Implies-block / quantifier-block** (indented body becomes `∧`-joined)
- **Chained comparisons** (`a ≤ b ≤ c` → `(a≤b) ∧ (b≤c)`)
- **`..ClaimName` passthrough** → inlined body items
- **Pin forms** (`x ∈ T (a ↦ 1, b ↦ 2)` → Membership + per-pin equalities)

Each is a transformation on the AST: input has shape X, output has
shape Y. Express each as a claim that relates `input_program ∈ Program`
and `desugared_program ∈ Program`. Run the desugar pass by querying.

This is where the bootstrap pays off most visibly. Today the Rust
parser is 1200 lines, much of it desugars. Half of that disappears
if desugaring moves into Evident.

### 4. Certain optimization passes

- **Constant folding**: `2 + 3` → `5`.
- **Dead-constraint elimination**: remove constraints that are
  trivially true under the current model.
- **Common subexpression detection**: identify identical sub-expressions
  for caching.
- **Lookup-table pre-flattening**: for tables whose keys are statically
  enumerable, replace `(k, v) ∈ table ⇒ body` with a series of
  per-key implications.

Optimizations need to be conservative (correct rewrites only); the
Evident representation makes correctness checks straightforward —
the original and rewritten programs should be observationally
equivalent, which is itself a constraint we can check.

### 5. Linting and style rules

- "This claim has 3 ClaimCalls with `mapsto` for slot names that
  match the caller — could use names-match composition."
- "This `∀ i ∈ {0..#seq-1}` could use `coindexed` / `edges`."
- "This declaration + assignment pair could use chained membership."

These are pattern-matching queries over the AST, returning the
locations and a suggested rewrite. The runtime's existing query +
sample machinery gives you the location dump for free.

---

## What's NOT Self-Hostable

Components that genuinely need procedural execution or low-level
machinery stay in the host runtime:

- **Lexer / tokenizer** — character-level state machine, not a
  constraint problem.
- **Z3 sort registry and value extraction** — interfacing with the
  external solver requires FFI or direct API calls.
- **I/O plugins** (Stdin, SDL, audio) — side-effecting code.
- **The executor's per-step loop** — needs imperative state
  management.
- **Performance-critical hot paths** that run per-frame (the cached
  query mechanism, structural-signature invalidation).
- **Debug-mode pretty printing** — needs to format errors cheaply.

The boundary is roughly: anything that reads/writes external state
or runs per-frame stays procedural. Anything that's pure transformation
on the AST is a candidate for self-hosting.

---

## Prerequisites

Several language features are gaps today (in the Rust runtime; see
`docs/rust-runtime-capabilities.md` §16) that block self-hosting:

1. **Named enum declarations** (`enum TypeKind = Int | Nat | Real | …`).
   The Rust parser doesn't recognize `enum` or `|` at the type level.
   The AST representation needs sum types, and the inference rules
   need them for the type lattice.

2. **Algebraic datatypes (sum types) for records.** `Expr` is one of
   `{Int, Bool, Identifier, Binary(BinOp, Box<Expr>, Box<Expr>), …}`.
   Z3 handles algebraic datatypes natively (the Rust runtime already
   builds them for `Seq(UserType)`), but the surface syntax doesn't
   yet expose this.

3. **A canonical `stdlib/ast.ev`** that defines `Program`, `SchemaDecl`,
   `BodyItem`, `Expr`, etc. as Evident types. This becomes the
   contract every runtime exposes when handing a parsed program to
   stdlib passes. Different runtimes can have internal AST
   representations, but they must be able to translate to this
   canonical form.

4. **A way to invoke a stdlib pass.** Probably a CLI subcommand:
   `evident infer-types <file>`, `evident desugar <file>`, etc.
   Each loads the relevant `stdlib/*.ev`, queries with the parsed
   AST as input, returns the result.

5. **Recursive types** in the AST. `Expr` contains nested `Expr`s.
   Z3 datatypes handle this; surface-level support exists in the
   Rust runtime for `Seq(UserType)` but recursive nesting may need
   verification.

(1)–(2) are the blocking gaps. (3)–(5) are infrastructure to be
built once (1) and (2) exist.

---

## Bootstrap Order

The self-hosted passes themselves are Evident programs. They have to
be loadable by a runtime that hasn't yet run type inference on them —
otherwise circular dependency. Two strategies:

**Option A — Fully explicit types in stdlib.** The stdlib passes are
written with every `∈ Type` explicit, no inference required. The
inference pass can then be self-hosted because it doesn't depend on
itself. This is the cleanest bootstrap — pay the verbosity cost once
in stdlib, reap the inference benefit everywhere else.

**Option B — Two-stage bootstrap.** Each runtime ships a minimal
hand-rolled stub that handles enough of the pass to load the full
self-hosted version. More flexibility but more host-code surface.

Option A is preferred. The stdlib already writes mostly-explicit
types; the marginal cost of going fully-explicit is small, and the
benefit of "no host code" is large.

---

## Performance

Self-hosted passes will be slower than hand-rolled equivalents.
Concretely:

- Type inference for a 1000-line program would mean encoding the
  AST as a Z3 datatype value and asserting hundreds of rules. That's
  seconds of solver time vs. microseconds for union-find.
- Each desugar pass adds at minimum one full Z3 query per program
  load.

This is **probably acceptable** for compile-time work — these passes
run once per program load, not per query or per frame. A 2-second
type-check beats a `panic!` from a missing type annotation. For
production paths (executing already-validated programs), the passes
can be cached or skipped via a flag.

If performance becomes blocking, the runtime can ship optional
hand-rolled fast paths for the common cases, falling back to the
self-hosted version for the long tail. The contract — what the pass
does — is defined by the Evident program; the implementation can vary.

---

## A Staged Path Forward

Each stage is shippable on its own and validates the next:

### Stage 0 — Close the language gaps
Add `enum`, named sum types, and verify recursive datatypes in the
Rust runtime. Without these, no self-hosting works.

### Stage 1 — Define `stdlib/ast.ev`
Specify `Program`, `SchemaDecl`, `BodyItem`, `Expr`, `BinOp`, etc.
as Evident types. The Rust runtime exposes a way to dump a parsed
program in this shape (probably as JSON, deserialized into the
Evident type at load time, or directly as a Z3 datatype value via
a new `--as-evident` parser flag).

### Stage 2 — Self-host the smallest desugar
Pick chained-membership (the most recent addition; ~200 lines of
Rust). Move it to `stdlib/desugar/chained_membership.ev`. Add CLI
support: `evident desugar --pass=chained-membership <file>`.
Validate that the output matches the procedural version on a
test corpus.

### Stage 3 — Self-host type inference (literal cases)
Start with the unambiguous trivials: `x = "hello"` → String,
`x = true` → Bool. Add a `--explain-types` flag that dumps the
inferred type per variable plus the rule that fired. This is the
visible win — users start writing programs with fewer explicit
type annotations.

### Stage 4 — Self-host more passes
Lookup-table completeness check. Unused-variable detection.
Common subexpression. One per release.

### Stage 5 — Migrate the rest of the desugars
Multi-name shorthand, chained comparisons, implies-block, etc.
Each one shrinks the Rust parser. Eventually, the parser does only
lexing + minimal AST construction; all transformations are stdlib
passes.

### Stage 6 — Document the runtime contract
Write `docs/runtime-implementation-spec.md` listing exactly what a
new Evident runtime must implement. Probably ~5 things:
parse Evident syntax, build the canonical AST, encode AST + Evident
constraints to Z3, query, format model values. Everything else is
stdlib.

---

## Why This Is The Right Bet

Evident's pitch is "models, not programs." The strongest possible
demonstration of that pitch is to build the *compiler itself* as
constraint models — every desugar rule, every type-inference
heuristic, every static check expressed as constraints rather than
procedures. If the language can't model its own implementation, the
pitch is weakened. If it can, the pitch becomes self-evident: you
read the type inferencer in 200 lines of Evident and understand it.

The cost is real (slower compile-time passes, language-feature
prerequisites, bootstrap discipline) but the benefits compound across
runtime implementations, language evolution, and user-facing
debuggability. For a language whose explicit goal is reducing the
implementation surface and lifting reasoning into models, this is
exactly the lift that proves the design.
