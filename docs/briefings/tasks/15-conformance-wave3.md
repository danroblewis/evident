# Task: Conformance migration wave 3 — Python tests → features/

## Why

`scripts/check-deletable.sh` reports 12 Python files. Most are in
`tests/conformance/test_*.py`. Migrate each to feature-spec format
under `tests/conformance/features/`, then delete the `.py`. Get
Python count down to just `tests/conformance/conftest.py` (which
stays until the last wave when `evident-self` replaces the
bootstrap binary path).

## Required reading

1. `tests/conformance/features/README.md` (spec format).
2. `tests/conformance/features/runner.sh`.
3. Existing features `001-…` through `016-…` for examples.
4. `tests/conformance/test_*.py` files — survey what each tests.

## What you're producing

For each `test_*.py` file (skip `conftest.py`):

1. Read the file. Identify each distinct property/behavior it
   tests.
2. Migrate each property to a new feature directory under
   `tests/conformance/features/0XX-<name>/`. Number sequentially
   from 017 onwards.
3. After ALL behaviors from a given `.py` are migrated, delete the
   `.py`.

Some tests may test things the feature-spec runner can't yet
express (e.g. testing for specific error messages on CLI). For
those:
- Note in the commit message what was skipped and why.
- Don't try to retrofit the runner — that's a different task.
- Delete the `.py` only if EVERY property in it migrated, OR if
  the un-migrated property is something we don't need.

## Acceptance

1. `tests/conformance/test_*.py` files deleted (except
   `conftest.py`).
2. `tests/conformance/features/` has new directories for every
   migrated property.
3. `IMPL=bootstrap tests/conformance/features/runner.sh` passes.
4. `./test.sh` is fully green (the legacy Python conformance
   phase will run fewer tests, but features/ replaces them).
5. `scripts/check-deletable.sh` Python count drops from 12 to 1
   (just `conftest.py`).

## Forbidden

- Editing `bootstrap/`, `kernel/`, `compiler/`, `stdlib/`.
- Editing `tests/conformance/conftest.py` (last to go).
- Adding Python.
- Reducing test coverage. If you can't migrate a property,
  document it; don't drop it silently.

## Reporting back

- Branch pushed.
- Per-`.py`-file disposition: migrated / partially migrated /
  deleted-no-migration-needed.
- Number of new features added (e.g. 017–025).
- `IMPL=bootstrap` runner summary.
- `./test.sh` final line.
- `scripts/check-deletable.sh` Python count before / after.

Be terse.
