# Evident — Project Invariants

## What This Is

Evident is a constraint programming language where programs are collections of
constraints over sets, and a Z3 SMT solver finds satisfying assignments.  The
central abstraction is `schema`: a named set defined by membership conditions.
Querying a schema asks whether a satisfying assignment exists.

## Language Definitions

| Thing | Where defined |
|---|---|
| Grammar (authoritative) | `parser/src/grammar.lark` |
| Unicode normalizer (∈→`__IN__` etc.) | `parser/src/normalizer.py` |
| AST node types | `parser/src/ast.py` |
| Lark → AST transformer | `parser/src/transformer.py` |
| Language spec (prose) | `spec/` (00-overview through 09-stdlib) |
| Design docs | `language-design.md`, `vision.md`, `models-not-programs.md` |
| Examples | `examples/` |

## Runtime Architecture

The runtime is a pipeline.  Each stage is a separate file under `runtime/src/`:

```
source text
  → normalizer.py        Unicode symbols → __TOKEN__ keywords
  → grammar.lark         Lark Earley parser
  → transformer.py       Lark tree → AST (ast.py nodes)
  → sorts.py             SortRegistry: maps type names to Z3 sorts
  → instantiate.py       Creates Z3 constants for schema variables;
                         expands sub-schema fields (task.duration, …)
  → translate.py         AST expressions/constraints → Z3 expressions
  → evaluate.py          EvidentSolver: runs the Z3 Solver, extracts model
  → runtime.py           EvidentRuntime: top-level API (load_source, query)
```

Supporting modules:
- `env.py` — immutable variable environment (name → Z3 expr)
- `quantifiers.py` — ∀ / ∃ constraint translation
- `compose.py` — names-match schema composition
- `evidence.py` — derivation trees returned from queries
- `sets.py` — set/array constraint translation
- `sorts.py` — Z3 sort registry; also owns enum variant name → constructor map
- `fixedpoint.py` — forward implication rules (A, B ⇒ C) via Z3 Fixedpoint
- `ast_types.py` — re-exports parser AST so runtime shares the same class objects
  (critical: isinstance checks break if two separate module instances exist)

## Keyword Conventions

All three keywords — `type`, `claim`, and `schema` — produce identical AST nodes
(`SchemaDecl`) and are interchangeable at the runtime level.  The distinction is
a reading contract described in `docs/design/what-we-learned.md`:

**`type`** — Use for things that define the structure of a single record value.
A type is a noun: something you instantiate and hold.  The constraints inside it
are simple local invariants on its own fields — always true for any valid instance,
no external dependencies.

```evident
type GameState
    location  ∈ String
    inventory ∈ Seq(Item)
    turn      ∈ Nat

type DateRange
    start ∈ Date
    end   ∈ Date
    start ≤ end        -- local invariant on DateRange's own fields
```

**`claim`** — Use for relations across multiple values, traits, properties, and
constraint modules.  A claim is a predicate: something that holds or doesn't hold
for a given set of values.  Claims are used both in test files (as assertions to
verify) and as constraint modules that can be mixed into other claims or types.
The test-file convention `sat_*` / `unsat_*` is just one application.

```evident
-- Trait / constraint module: a reusable property
claim assignment_fits_schedule
    a        ∈ Assignment
    schedule ∈ Set Assignment
    ∀ b ∈ schedule : a.room = b.room ⇒ a.slot ≠ b.slot

-- Test assertion
claim sat_north_exit_exists
    ("entrance", "north", "forest") ∈ exits_map
```

The practical line: if the constraints are purely local to the type's own fields
→ `type`.  If they involve external data, multiple objects, or complex logic that
varies by context → `claim`.

**`schema`** — Avoid.  It is a synonym for `type` with no additional meaning.
Prefer `type` when the thing is a noun (has a shape); prefer `claim` when it is a
predicate (defines a relation or property).  The word `schema` does not appear in
human-written Evident source files.

**`..TypeName` (passthrough / trait composition)** — Brings another type's or
claim's fields and constraints directly into the current scope without a dotted
prefix.  Think of it as trait composition.  The included declaration is still a
`type` or `claim`; `..` is the composition mechanism.

## Composing Types and Claims

### Using a type inside a claim: `variable ∈ TypeName`

Declares a variable of that type.  All of the type's fields become accessible
as `variable.field`, and the type's invariants are automatically enforced.
Use this when a claim needs to reason about a structured object.

```evident
claim assignment_fits_schedule
    a        ∈ Assignment      -- a is an Assignment; a.room, a.slot available
    schedule ∈ Set Assignment
    ∀ b ∈ schedule : a.room = b.room ⇒ a.slot ≠ b.slot
```

### Using a claim inside a type: baking a property in

When every instance of a type should satisfy a property, put the claim's
name directly in the type body.  The names-match rule identifies variables
automatically.

```evident
type ValidSchedule
    slots   ∈ Seq(TimeSlot)
    budget  ∈ Nat
    no_conflicts     -- claim; 'slots' matches by name
    within_budget    -- claim; 'budget' matches by name
```

This creates a **refined type** — a subset of all schedules that satisfy
those additional properties.  Use it when the constraint should always hold
for any valid instance, with no external data needed.

### Passthrough `..`: flat mixin, no prefix

`..SomeType` or `..SomeClaim` brings all fields into the current scope
without a dotted prefix.  The included constraints also apply.

```evident
type main
    ..LineReader    -- adds line, line_ready, src.* directly into scope
    ..LineWriter    -- adds line_out, dst.* directly into scope
    state ∈ GameState
```

Use passthrough when the fields of the included type/claim ARE fields of
the current type — not a sub-object you reference by name.  `..LineReader`
makes `line` available directly; `reader ∈ LineReader` would make it
`reader.line`.

### Names-match composition: zero-argument claims

When variable names in scope match a claim's variable names, just name the
claim — no explicit argument passing needed.  The solver identifies them.

```evident
claim valid_conference
    schedule     ∈ Set Assignment
    rooms        ∈ Set Room
    max_parallel ∈ Nat

    rooms_conflict_free    -- 'schedule' flows automatically by name
    parallel_load_within   -- 'schedule', 'max_parallel' flow by name
```

### Renaming with `↦`: when names differ

```evident
claim manage_event
    assignments ∈ Set Assignment
    Conference.valid (schedule ↦ assignments)  -- rename to match
```

### Decision guide

| Situation | Pattern |
|---|---|
| A claim needs one structured object | `variable ∈ TypeName` in the claim |
| A type should always satisfy a property | name the claim in the type body |
| Fields should live flat in scope (no prefix) | `..TypeName` or `..ClaimName` |
| Reusing a claim whose variable names match | name it directly (names-match) |
| Reusing a claim with different variable names | name it with `(x ↦ y)` |
| A subset of a type with extra invariants | define a new `type` that names the original type and adds constraints |

## Key Invariants

**Parser**
- The grammar is the single source of truth for syntax.  The normalizer runs
  first and converts Unicode operators to `__TOKEN__` form before Lark sees the
  source, so the grammar only contains ASCII tokens for operators.
- `normalizer.py` maps both directions: Unicode symbols *and* word keywords
  (`in`, `not in`, `subset`, `superset`, `mapsto`) to the same `__TOKEN__`.
  Adding a new keyword requires updating the normalizer *and* the grammar.

**AST**
- Runtime files import AST types from `runtime/src/ast_types.py`, not directly
  from `parser/src/ast.py`.  `ast_types.py` re-exports via a proper package
  import so all code shares one class identity — two separate `importlib.util`
  loads produce different class objects and break `isinstance`.

**Sorts and enums**
- `SortRegistry` is the single owner of all Z3 sorts and enum constructors.
- Enum variant names are **global** and must be unique across all enum types.
  `declare_algebraic` raises `ValueError` on duplicate variant names.
- `type Color = Red | Green | Blue` declares a named enum.
- `x ∈ Red | Green | Blue` (inline enum) auto-declares an anonymous enum named
  `_Enum_<sorted_variants>` and is equivalent to declaring the type separately.

**Variable scoping**
- Variables declared inside a schema (`x ∈ Nat`) are local to that schema's
  query.  Independent queries do not share environments.
- Composed sub-schemas get a dotted prefix: `task ∈ Task` expands into
  `task.id`, `task.duration`, etc. in the parent environment.  The bare `task`
  variable is not created; only the leaf fields exist in Z3.
- Type names (e.g. `Color`) can be reused as variable names inside a schema
  without conflict — they occupy different namespaces.

**Z3 safety**
- Z3's C library is not safe for concurrent use from multiple threads.
- The IDE backend runs `/sample` and `/ranges` in isolated subprocesses via
  `ide/backend/z3_worker.py` to prevent server crashes.
- `/ranges` results are cached (LRU, 128 entries) keyed by request hash.
  `/sample` is intentionally **not** cached — results are random.
- Push/pop inside a single subprocess is safe.  Never use push/pop across
  request boundaries in the web server process.

**Sub-schema field access**
- `task.duration` in source is parsed as `BinaryExpr(×, Identifier('task'),
  FieldAccess('.', 'duration'))` by the grammar (juxt-dot ambiguity).
  `translate.py` intercepts this pattern and resolves it as a dotted env
  lookup before evaluating operands.

## IDE

```
ide/
  backend/
    main.py          FastAPI app; /parse, /evaluate, /ranges, /sample, /transfer
    z3_worker.py     Subprocess worker for Z3 isolation
    ranges.py        Binary-search minimum finder (no Z3 Optimize)
    sampler.py       blocking_clause_sample, random_seed_sample, grid_sample
  frontend/
    editor.js        Monaco setup + LaTeX-style keyword→symbol substitution
    evident-lang.js  Monaco Monarch tokenizer + dark theme
    schema-panel.js  Schema selector and variable binding inputs
    samples.js       Sample table; accumulates unique samples across runs
    ranges.js        Variable range bars
    scatter.js       2D plot: scatter (num×num), strip (enum×num), count bars (enum)
  tests/
    test_ide.py      Playwright end-to-end tests (server must be on :8765)
```

**Running the IDE**

```bash
uvicorn ide.backend.main:app --port 8765
# then open http://localhost:8765/app/
```

**Running tests**

```bash
pytest runtime/tests/ parser/tests/     # unit tests (fast, ~2s)
pytest ide/tests/test_ide.py            # Playwright e2e (requires server on :8765)
```
