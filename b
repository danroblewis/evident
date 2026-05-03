     1	# Evident — Project Invariants
      	
     2	## What This Is
      	
     3	Evident is a constraint programming language where programs are collections of
     4	constraints over sets, and a Z3 SMT solver finds satisfying assignments.  The
     5	central abstraction is `schema`: a named set defined by membership conditions.
     6	Querying a schema asks whether a satisfying assignment exists.
      	
     7	## Language Definitions
      	
     8	| Thing | Where defined |
     9	|---|---|
    10	| Grammar (authoritative) | `parser/src/grammar.lark` |
    11	| Unicode normalizer (∈→`__IN__` etc.) | `parser/src/normalizer.py` |
    12	| AST node types | `parser/src/ast.py` |
    13	| Lark → AST transformer | `parser/src/transformer.py` |
    14	| Language spec (prose) | `spec/` (00-overview through 09-stdlib) |
    15	| Design docs | `language-design.md`, `vision.md`, `models-not-programs.md` |
    16	| Examples | `examples/` |
      	
    17	## Runtime Architecture
      	
    18	The runtime is a pipeline.  Each stage is a separate file under `runtime/src/`:
      	
    19	```
    20	source text
    21	  → normalizer.py        Unicode symbols → __TOKEN__ keywords
    22	  → grammar.lark         Lark Earley parser
    23	  → transformer.py       Lark tree → AST (ast.py nodes)
    24	  → sorts.py             SortRegistry: maps type names to Z3 sorts
    25	  → instantiate.py       Creates Z3 constants for schema variables;
    26	                         expands sub-schema fields (task.duration, …)
    27	  → translate.py         AST expressions/constraints → Z3 expressions
    28	  → evaluate.py          EvidentSolver: runs the Z3 Solver, extracts model
    29	  → runtime.py           EvidentRuntime: top-level API (load_source, query)
    30	```
      	
    31	Supporting modules:
    32	- `env.py` — immutable variable environment (name → Z3 expr)
    33	- `quantifiers.py` — ∀ / ∃ constraint translation
    34	- `compose.py` — names-match schema composition
    35	- `evidence.py` — derivation trees returned from queries
    36	- `sets.py` — set/array constraint translation
    37	- `sorts.py` — Z3 sort registry; also owns enum variant name → constructor map
    38	- `fixedpoint.py` — forward implication rules (A, B ⇒ C) via Z3 Fixedpoint
    39	- `ast_types.py` — re-exports parser AST so runtime shares the same class objects
    40	  (critical: isinstance checks break if two separate module instances exist)
      	
    41	## Key Invariants
      	
    42	**Parser**
    43	- The grammar is the single source of truth for syntax.  The normalizer runs
    44	  first and converts Unicode operators to `__TOKEN__` form before Lark sees the
    45	  source, so the grammar only contains ASCII tokens for operators.
    46	- `normalizer.py` maps both directions: Unicode symbols *and* word keywords
    47	  (`in`, `not in`, `subset`, `superset`, `mapsto`) to the same `__TOKEN__`.
    48	  Adding a new keyword requires updating the normalizer *and* the grammar.
      	
    49	**AST**
    50	- Runtime files import AST types from `runtime/src/ast_types.py`, not directly
    51	  from `parser/src/ast.py`.  `ast_types.py` re-exports via a proper package
    52	  import so all code shares one class identity — two separate `importlib.util`
    53	  loads produce different class objects and break `isinstance`.
      	
    54	**Sorts and enums**
    55	- `SortRegistry` is the single owner of all Z3 sorts and enum constructors.
    56	- Enum variant names are **global** and must be unique across all enum types.
    57	  `declare_algebraic` raises `ValueError` on duplicate variant names.
    58	- `type Color = Red | Green | Blue` declares a named enum.
    59	- `x ∈ Red | Green | Blue` (inline enum) auto-declares an anonymous enum named
    60	  `_Enum_<sorted_variants>` and is equivalent to declaring the type separately.
      	
    61	**Variable scoping**
    62	- Variables declared inside a schema (`x ∈ Nat`) are local to that schema's
    63	  query.  Independent queries do not share environments.
    64	- Composed sub-schemas get a dotted prefix: `task ∈ Task` expands into
    65	  `task.id`, `task.duration`, etc. in the parent environment.  The bare `task`
    66	  variable is not created; only the leaf fields exist in Z3.
    67	- Type names (e.g. `Color`) can be reused as variable names inside a schema
    68	  without conflict — they occupy different namespaces.
      	
    69	**Z3 safety**
    70	- Z3's C library is not safe for concurrent use from multiple threads.
    71	- The IDE backend runs `/sample` and `/ranges` in isolated subprocesses via
    72	  `ide/backend/z3_worker.py` to prevent server crashes.
    73	- `/ranges` results are cached (LRU, 128 entries) keyed by request hash.
    74	  `/sample` is intentionally **not** cached — results are random.
    75	- Push/pop inside a single subprocess is safe.  Never use push/pop across
    76	  request boundaries in the web server process.
      	
    77	**Sub-schema field access**
    78	- `task.duration` in source is parsed as `BinaryExpr(×, Identifier('task'),
    79	  FieldAccess('.', 'duration'))` by the grammar (juxt-dot ambiguity).
    80	  `translate.py` intercepts this pattern and resolves it as a dotted env
    81	  lookup before evaluating operands.
      	
    82	## IDE
      	
    83	```
    84	ide/
    85	  backend/
    86	    main.py          FastAPI app; /parse, /evaluate, /ranges, /sample, /transfer
    87	    z3_worker.py     Subprocess worker for Z3 isolation
    88	    ranges.py        Binary-search minimum finder (no Z3 Optimize)
    89	    sampler.py       blocking_clause_sample, random_seed_sample, grid_sample
    90	  frontend/
    91	    editor.js        Monaco setup + LaTeX-style keyword→symbol substitution
    92	    evident-lang.js  Monaco Monarch tokenizer + dark theme
    93	    schema-panel.js  Schema selector and variable binding inputs
    94	    samples.js       Sample table; accumulates unique samples across runs
    95	    ranges.js        Variable range bars
    96	    scatter.js       2D plot: scatter (num×num), strip (enum×num), count bars (enum)
    97	  tests/
    98	    test_ide.py      Playwright end-to-end tests (server must be on :8765)
    99	```
      	
   100	**Running the IDE**
      	
   101	```bash
   102	uvicorn ide.backend.main:app --port 8765
   103	# then open http://localhost:8765/app/
   104	```
      	
   105	**Running tests**
      	
   106	```bash
   107	pytest runtime/tests/ parser/tests/     # unit tests (fast, ~2s)
   108	pytest ide/tests/test_ide.py            # Playwright e2e (requires server on :8765)
   109	```
