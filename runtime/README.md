# runtime — Evident runtime, Rust port

Experimental Rust port of the Python runtime in `runtime/src/`. Goal is
not feature parity (yet) — it's to validate that the architecture
translates cleanly and to get a feel for what's involved.

## What's the same as the Python runtime

The pipeline shape is identical. Each Rust module mirrors a Python file:

| Python module           | Rust module           | Purpose                                |
|-------------------------|-----------------------|----------------------------------------|
| `parser/src/normalizer` | `src/lexer.rs`        | Unicode (∈, ∧, …) → tokens             |
| `parser/src/grammar`    | `src/parser.rs`       | Tokens → AST                           |
| `parser/src/ast.py`     | `src/ast.rs`          | AST node types                         |
| `runtime/src/sorts`     | `src/sorts.rs`        | Type → Z3 sort registry                |
| `runtime/src/instantiate` | (folded into translate) | Declare Z3 constants               |
| `runtime/src/translate` | `src/translate.rs`    | AST → Z3 expressions                   |
| `runtime/src/evaluate`  | `src/evaluate.rs`     | Solver wrapper                         |
| `runtime/src/runtime`   | `src/runtime.rs`      | Top-level API                          |

Z3 is the same backend, accessed via the `z3` crate (which links the
same C++ library Python's `z3-solver` package binds to).

## What's intentionally cut for v0.1

- Plugins / executor loop / SDL — runtime only, no I/O loop.
- Subclaims, claim composition, passthrough, mappings.
- Quantifiers (∀, ∃) — even unrolling is non-trivial.
- Composite types in Set/Seq.
- Sequences and Strings (mostly).
- The cached-evaluator optimization.
- Evidence trees / unsat-core explanations.

## v0.1 scope (target)

Make this Python test pass against the Rust runtime:

```python
schema SimpleNat
    n ∈ Nat
    n > 5
```

i.e. parse a `schema` block with a typed declaration and a numeric
constraint, run a query, and return a model with `n > 5`.

Once that works, grow the supported subset constraint by constraint,
guided by the Python `runtime/tests/test_end_to_end.py` file.

## Layout

```
runtime/
├── README.md          ← you are here
├── PROGRESS.md        ← live status; check first when resuming
├── NOTES.md           ← Evident invariants worth remembering
├── Cargo.toml
├── src/
└── tests/             ← Rust integration tests mirroring Python ones
```

## Build / run

```bash
cd runtime
cargo build
cargo test
```

Z3 is required. On macOS: `brew install z3`. The `z3` crate uses
`Z3_SYS_Z3_HEADER` to find the headers if not in standard locations.

## `evident` CLI

Mirrors `evident.py`'s subcommand shape. Implemented:

```bash
evident query  <files…> <schema> [--given k=v …] [--json]
evident check  <files…>
evident sample <files…> <schema> [-n N] [--given k=v …] [--json]
evident test   [path]
evident parse  <file>     # debug helper, Rust-only
```

Parked behind plugin/executor work (use `evident.py` for now):

```bash
evident execute|batch|repl …    # → "not yet implemented" (exit 2)
```

Build via cargo (uses .cargo/config.toml's DYLD vars):

```bash
cargo run --quiet -- query path/to/schema.ev SchemaName
cargo run --quiet -- check path/to/schema.ev
cargo run --quiet -- test programs/                 # walks for test_*.ev
```

Or the standalone binary (build script patches libz3's load command —
`libz3.dylib`'s install_name is bare and dyld won't search rpath for
it on macOS, so `install_name_tool -change` rewrites it post-link):

```bash
./scripts/install-bin.sh                  # build + patch in place
./target/release/evident query path/to/schema.ev SchemaName
./scripts/install-bin.sh /usr/local/bin   # also copy to a dest dir
```

Output:
  - `query` SAT  → `KEY=VALUE` lines (sorted), exit 0
  - `query` UNSAT → `UNSAT`, exit 1
  - `--json`      → `{"satisfied": …, "bindings": {…}}`
  - `check`       → `SAT|UNSAT|ERROR  <name>` per schema; exit 1 if any UNSAT
  - `test`        → `PASS|FAIL  <name>` per `sat_*` / `unsat_*` claim,
                    plus a final `N passed, M failed, K skipped` summary
