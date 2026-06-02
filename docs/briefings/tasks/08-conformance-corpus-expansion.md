# Task: Migrate 10 more conformance tests from Python to features/

## Why

`tests/conformance/features/` currently has 6 spec-format tests
(from task #02). The 14 remaining Python tests
(`tests/conformance/test_*.py`) keep running but are on the
deletion path. Each test we migrate to the spec format is one
Python file we can eventually delete AND one more equivalence
check we'll have under `IMPL=both` once `compiler.smt2` exists.

## Required reading

1. `tests/conformance/features/README.md` — the spec format.
2. `tests/conformance/features/runner.sh` — how the runner
   handles `bootstrap` / `selfhost` / `both`.
3. A handful of existing features (e.g. `001-int-arithmetic-add/`)
   to see the pattern.
4. `docs/plans/DELETION-CHECKLIST.md` Phase 1.

## What you're producing

Pick 10 small, focused tests from `tests/conformance/test_*.py` and
migrate each. Choose ones that test ONE feature each, not large
multi-aspect tests. Good candidates: tests around single language
features (string ops, comparisons, simple matches, simple enums,
ternary, basic Seq operations).

Each migrated test becomes a directory under
`tests/conformance/features/` with the spec format:

```
features/00N-<descriptive-name>/
  source.ev
  claim.txt
  expected/
    smt2-contains    # at least one substring that must appear in the .smt2
    stdout?          # optional, kernel stdout when running the .smt2
    exit?            # optional, kernel exit code
```

Number new features starting at `007-` (the existing ones are 001
through 006).

## Acceptance

- 10 new features under `tests/conformance/features/007-…` etc.
- `IMPL=bootstrap tests/conformance/features/runner.sh` reports
  16/16 passing (existing 6 + new 10).
- `./test.sh` green.
- The Python tests you derived these from are NOT deleted yet —
  they keep running until the next wave. Just MIRROR their
  behaviour into the new spec format.

## Forbidden

- Editing `bootstrap/`, `kernel/`, `compiler/`, `stdlib/`.
- Adding new `.py` files.
- Editing existing `.py` files.
- Deleting Python tests (later wave).
- Picking 10 tests that overlap (e.g. 3 different "string concat"
  variants). Distinct features.

## Reporting back

- Branch pushed.
- Names of the 10 features you created.
- `IMPL=bootstrap tests/conformance/features/runner.sh` final summary.
- `./test.sh` final line.

Be terse.
