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
