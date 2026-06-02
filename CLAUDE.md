# Evident — Project Invariants

## What This Is

Evident is a constraint programming language where programs are
collections of constraints over sets, and a Z3 SMT solver finds
satisfying assignments. The central abstraction is `schema` (or
`type` / `claim`): a named set defined by membership conditions.
Querying a schema asks whether a satisfying assignment exists.

## Scope of this runtime

The Rust runtime in `runtime/` is **the language frontend + Z3 IR**.
It does only three things:

1. **Language syntax & semantics** — lexer, parser, AST.
2. **Claim composition** — `..passthrough`, names-match, explicit
   binding `↦`, guarded `⇒`, tuple-in-claim, subclaim, chained
   membership `0 < x ∈ Int < 5`.
3. **Z3 model construction** — translating the AST into Z3 sorts,
   constraints, and solver calls; extracting model values.

The runtime is **not** an effect-driven program executor. There is
no multi-FSM scheduler, no async event sources, no FFI/libcall
dispatch, no self-hosted pass runner. Earlier iterations had all of
those; they were removed because they couple "the language" to
"running programs with side effects" — a much bigger contract.

If you find yourself wanting to execute a program (drive an FSM,
emit effects, call C), build that on top of `EvidentRuntime::query`
in a separate crate.

## How to run tests

Single command, ~3 seconds:

```
./test.sh
```

Phases:
1. **build** — `cargo build --release`
2. **cargo test** — Rust unit tests
3. **conformance** — `pytest tests/conformance/` — black-box CLI tests
4. **lang_tests** — `scripts/run-lang-tests.py` drives every
   `tests/lang_tests/*.ev` through `evident sample --all --json` and
   asserts each `sat_*` claim is SAT, each `unsat_*` is UNSAT.

Flags:
- `./test.sh --rust-only` — skip Python phases
- `./test.sh --conformance` — only conformance
- `./test.sh --lang` — only language tests

Always run `./test.sh` before declaring work done.

## CLI

One subcommand:

```
evident sample <file> <claim> [-n N] [--json] [--given k=v ...]
evident sample <file> --all [--json]
```

- `sample <file> <claim>` runs `query` on a single claim, prints
  satisfied + bindings.
- `sample <file> --all --json` sat-checks every loaded schema and
  emits `{"name": bool, …}` as JSON.
- `--given k=v` pre-binds a variable; values parse as `Int`, `Bool`
  (`true`/`false`), `Real`, or `String`.

Generic templates (`type Edge<T>`) are skipped by `--all` — they
don't translate on their own; only their monomorphized copies do.

## Source layout

```
runtime/
├── Cargo.toml
└── src/
    ├── core/             — AST + shared types (Value, Var, EnumRegistry, …)
    ├── lexer.rs          — Unicode operators → tokens
    ├── parser/           — Tokens → AST (recursive-descent)
    ├── translate/        — AST → Z3 ASTs, build solvers, extract models
    │   ├── declare.rs    — Per-type Z3 leaf declarations
    │   ├── preprocess.rs — Pin literal ints, propagate Seq lengths
    │   ├── exprs/        — Per-expression-kind translators
    │   ├── inline/       — Claim-composition inliners
    │   ├── eval/         — Top-level `evaluate` + cached path
    │   ├── encode_ast.rs — Value::Enum → Z3 Datatype (for enum pins)
    │   ├── extract.rs    — Z3 model → Rust Value
    │   └── datatypes.rs  — Seq(UserType) datatype caching
    ├── runtime/          — Top-level API: load, query, monomorphize
    │   ├── mod.rs        — EvidentRuntime struct
    │   ├── load.rs       — Parse + run passes + cache flush
    │   ├── query.rs      — public query + query_cached
    │   ├── desugar.rs    — `++` Seq concat flattening
    │   ├── inject.rs     — Type inference (claim-arg + lhs-eq)
    │   ├── generics.rs   — Generic monomorphization
    │   └── register_enums.rs — Z3 datatype registration
    ├── lib.rs            — Public surface
    ├── main.rs           — CLI
    └── z3_ctx.rs         — Global Z3 Context mutex (thread-safety)

stdlib/
├── combinatorics.ev      — Distinct, sorted, etc.
└── toposort.ev           — Topological sort claim

tests/
├── lang_tests/           — *.ev files with sat_*/unsat_* claims
└── conformance/          — Python black-box CLI tests
```

## Language reference

### Keywords

Four schema-introducing keywords — `type`, `claim`, `schema`, `fsm` —
all produce the same AST node (`SchemaDecl`). The distinction is a
reading contract:

- `type` — defines the structure of a single record value (a noun).
- `claim` — defines a predicate/relation/property (a verb-like
  assertion). Test files use `sat_*` / `unsat_*` claim names.
- `schema` — synonym for `type`. Prefer `type` in new code.
- `fsm` — present in the grammar but not load-bearing in this
  runtime. Treat it as another synonym for `claim`. (Earlier
  runtimes used `fsm` to auto-instantiate the multi-FSM scheduler;
  that scheduler is gone.)

### Composition

| Form | Meaning |
|---|---|
| `variable ∈ TypeName` | Declares a variable of the named type, with its fields and invariants |
| `..ClaimName` | Inline the claim's body via names-match (fields become flat in scope) |
| `ClaimName` (bare) | Inline the claim's body via names-match (synonym; resolved at translation) |
| `ClaimName(slot ↦ val)` | Inline with explicit slot binding |
| `(a, b) ∈ ClaimName` | Inline with positional binding to first-line params |
| `cond ⇒ ClaimName` | Conditional inline (constraints wrapped in `cond ⇒ …`) |
| `recv.subclaim(args)` | Subclaim dispatch with receiver-prefix |
| `subclaim Name` | Nested claim registered as a top-level schema |
| `..TypeName` (in another type) | Trait-like flat mixin of fields + invariants |

### Chained membership

```evident
x ∈ Int = 5            -- declare + pin
x ∈ Int < 10           -- declare + upper bound
0 < x ∈ Int < 10       -- declare + range
a, b, c ∈ Int < 5      -- multi-name (3 decls, each bounded)
```

### Records & lift forms

Define short records once:
```evident
type IVec2(x, y ∈ Int)
type Color(r, g, b ∈ Nat)
```

Then four lifts work automatically:
1. **Componentwise comparison** — `a < b`, `a = b`, `lo ≤ x ≤ hi`
2. **Arithmetic broadcast** — `c = a - b`
3. **Type-use pins** — `pos ∈ IVec2(380, 280)` or `pos ∈ IVec2(x ↦ 1)`
4. **Record literals in expressions** — `state.pos = IVec2(0, 0)`

### Seq

```evident
items ∈ Seq(Int) = ⟨1, 2, 3⟩       -- literal
xs ∈ Seq(Int) = a ++ b ++ ⟨c⟩       -- `++` flattens at load time
#items = 3                          -- cardinality
∀ x ∈ items : x > 0                 -- element iteration
∀ (cur, nxt) ∈ coindexed(a, b) : …  -- parallel zip
∀ (a, b) ∈ edges(seq) : …           -- consecutive pairs
```

### Enums

```evident
enum Color = Red | Green | Blue
enum Result = Ok(Int) | Err(String)
enum LL = Nil | Cons(Int, LL)
enum A = X(B) ; enum B = Y(A)       -- forward refs + mutual recursion
```

Variant names are globally unique across all enums.

### Match & matches

```evident
n = match e
    Ok(v) ⇒ v
    Err(_) ⇒ 0

is_ok = e matches Ok(_)             -- Bool recognizer
```

### Generic types & claims

```evident
type Edge<T>(from, to ∈ T)
claim Toposort<T>
    n ∈ Nat
    items ∈ Seq(T)
    edges ∈ Seq(Edge<T>)
    sorted ∈ Seq(T)
    -- … body relating items/edges/sorted via T …

es ∈ Seq(Edge<Rect>)
Toposort<Rect>(n ↦ 4, items ↦ rects, …)
```

Type-parameter names are capitalized to disambiguate from comparison
operators. Explicit type args only — no inference at call sites yet.

### Boolean & precedence footguns

- `true` / `false` are lowercase. `True` parses as an unbound name,
  silently dropping the constraint.
- `⇒` binds tighter than `∧` — `A ⇒ B ∧ C` parses as `(A ⇒ B) ∧ C`.
  Wrap compound consequents: `A ⇒ (B ∧ C)`.
- `=` binds tighter than `∧` / `∨` and comparisons. Wrap boolean
  assignments: `flag = (x < 5 ∧ y > 0)`.

### Type inference (dropped annotations)

The runtime recovers types from the RHS in most cases:
```evident
ok = (x > 0)                        -- Bool from comparison
mid = (n > 0 ? n : 0 - n)           -- Int from ternary arms
sky = Color(80, 160, 220)           -- Color from ctor
target = _world.pos                  -- IVec2 from field type
```

What stays explicit: top-level literal pins (`x = 5` needs
`x ∈ Int = 5`), and `type` body memberships.

## Idioms to avoid

- **Parallel Seqs.** If you find yourself with `from ∈ Seq(Int)` and
  `to ∈ Seq(Int)` that are supposed to line up, use a record:
  `type Edge(from, to ∈ Int)` + `edges ∈ Seq(Edge)`. Z3 silently
  fills in unconstrained values; misaligned parallel Seqs become
  silent wrong-answer bugs.
- **Indices in interfaces.** If a claim's input or output uses
  `Int` indices to identify "which item", you're leaking an
  implementation choice. Domain types in, domain types out.
- **Stacked ternaries.** Three ternaries that all hardcode the same
  number = an entity system asking to be defined. Promote the
  boundary to a record and let the constraint do the work.
- **Range-of-indices quantifiers.** Prefer `∀ x ∈ seq : …` over
  `∀ i ∈ {0..#seq - 1} : … seq[i] …`. The element form matches
  the math.

## Tests as documentation

`tests/lang_tests/*.ev` is the language-truth set. Each file is
named `test_<topic>.ev` and contains `claim sat_*` / `claim
unsat_*` declarations — the runner asserts on the SAT prefix.

| File | Tests |
|---|---|
| `test_chained_membership.ev` | `x ∈ T = …` / `… < x ∈ T < …` etc. |
| `test_cons_chain_lit.ev` | `⟨a, b, c⟩` on user Cons/Nil enums |
| `test_enums_basic.ev` | Nullary enum variants |
| `test_enums_payload.ev` | Payload variants, self-recursion |
| `test_enums_mutual.ev` | Forward refs, mutual recursion |
| `test_match.ev` | `match` expression |
| `test_matches.ev` | `matches` Bool recognizer |
| `test_record_lit_arg.ev` | Record literal as claim arg |
| `test_ternary.ev` | `c ? a : b` |
| `test_tuple_in_claim.ev` | `(a, b) ∈ ClaimName` |

When fixing a language bug, prefer adding a sat/unsat claim to one
of these files over a fresh test file.

## What got removed and why

(The audit is in `docs/rust-runtime-justification.md`.)

- **Multi-FSM scheduler + async event sources** — coupled the
  runtime to "running programs", not "modeling sets of programs".
- **FFI / libcall / SDL+GL bindings** — same reason.
- **Self-hosted pass runner** — passes were written in Evident that
  walked Evident ASTs; the passes that survived (generics
  monomorphization, `++` flattening, type inference) are now small
  Rust functions.
- **Reflection / introspect / autotune / lenient** — diagnostic
  knobs the language doesn't depend on.
- **examples/** — every example exercised the multi-FSM runtime.

## Authoring style for Evident source

- Drop annotations the inference recovers.
- Default to no comments. Add one only when *why* isn't obvious.
- Record types over parallel Seqs.
- Element-form iteration over index ranges.
- A compact entry-point reads as wiring; logic lives in claims.
