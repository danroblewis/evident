# Evident IDE

Tooling for the Evident web IDE goal loop: the task ledger (`task.py`),
the web server (`web/server.py`), and the front-end (`web/static/app.js`).

## Linting

`ide/lint.py` is a standalone, dependency-free (stdlib `ast` only) code-quality
linter for the IDE's Python codebase. It guards against the code getting
unwieldy by enforcing four mechanical, configurable rules. It **reports only** —
it never edits or reformats a scanned file.

### Usage

```bash
python3 ide/lint.py            # full report grouped by rule; exits non-zero on any violation
python3 ide/lint.py --quiet    # summary counts only
```

Run it from the repo root. Locations print as `path:line` so they're clickable.
The non-zero exit on any violation means it can gate CI.

### What it scans

`ide/*.py`, `ide/**/*.py`, and `viz/*.py` (excluding `__pycache__` and `lint.py`
itself). `ide/web/static/app.js` is JavaScript, so only the line-count rule
applies to it (added via the `JS_EXTRA_FILES` list).

### Rules and thresholds

Every threshold is a single editable constant at the top of `lint.py`.

| # | Rule | Constant | Default | Rationale |
|---|------|----------|---------|-----------|
| 1 | Max file line count | `MAX_FILE_LINES` | 500 | Matches CLAUDE.md's "≤ 500 lines per file". |
| 2 | Max function/method length | `MAX_DEF_LINES` | 70 | `end_lineno − lineno + 1`; a long `def` is a "do one thing" smell. |
| 3 | Max module-level (free) functions | `MAX_MODULE_FUNCS` | 12 | Many top-level `def`s = logic that wants to be a class or a split. |
| 4 | Module↔module coupling | `MAX_COUPLING` | 8 | See heuristic below. |

**Rule 4 heuristic** (mechanical, low-false-positive): per file,
`score = (# distinct sibling-module imports) + 2 × (# private cross-module accesses)`.

- A *sibling-module import* is an edge to one of **our own** packages — `from
  ide.x import …`, `import viz.y`, or a relative `from .z import …`. Stdlib and
  third-party imports (matplotlib, numpy, …) are **not** counted: they say
  nothing about internal tangle.
- A *private cross-module access* is reaching into another module's `_`-prefixed
  name through an imported alias (`othermod._helper(...)`) — a deliberate
  encapsulation break, so it's weighted ×2.

A file over `MAX_COUPLING` is flagged. The set of "our" packages is
`OUR_PACKAGES = {"ide", "viz"}`.

### Suppression

Put `# noqa: lint` on a line to suppress every violation reported at that line
(e.g. on a `def` line to grandfather one over-long function).

### Adjusting

Edit the constants at the top of `lint.py`. There's no config file by design —
the thresholds are few and live with the checker.
