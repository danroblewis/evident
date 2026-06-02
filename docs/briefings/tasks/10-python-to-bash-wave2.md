# Task: Python→Bash migration, wave 2 (test runners)

## Why

Wave 1 (task #07) removed `runtime-size.py` and `strip-comments.py`,
dropping Python count from 17 to 15. Wave 2 targets the test-runner
scripts that `test.sh` invokes directly, getting us closer to a
test pipeline with no Python on the critical path.

## Targets (this wave)

1. `scripts/run-kernel-tests.py` — runs `tests/kernel/*.ev` through
   `evident emit` + `kernel`, checks `-- expect:` headers. This is
   ON the `test.sh` critical path; replacement must produce
   identical output.
2. `scripts/run-lang-tests.py` — runs `tests/lang_tests/*.ev`
   through `evident sample --all --json`, asserts `sat_*`/`unsat_*`
   prefixes. Same: on critical path; must match output.
3. `scripts/lexer-oracle.py` — A Phase-A acceptance harness
   comparing Rust lexer to Evident lexer. NOT on `test.sh` critical
   path. Safe to delete entirely if you cannot easily replicate in
   Bash — but document the deletion.

## What you're producing

For (1) and (2):
- `scripts/run-kernel-tests.sh` and `scripts/run-lang-tests.sh`
  with `# TODO: rewrite in Evident` headers.
- Each must produce output equivalent to the Python version on the
  current test corpus (the existing `tests/kernel/*.ev` and
  `tests/lang_tests/*.ev`).
- Update `test.sh` to call the `.sh` versions instead of the `.py`.
- Delete the `.py` files.

For (3):
- Either implement a Bash equivalent OR delete the file with a
  one-paragraph note in the commit message explaining why it's
  safe to drop. (No `test.sh` phase invokes it.)

## Acceptance

- `scripts/run-kernel-tests.sh` and `scripts/run-lang-tests.sh`
  exist and work.
- `test.sh` calls the `.sh` versions; the corresponding `.py`
  files are deleted.
- `scripts/lexer-oracle.py` either replaced or deleted.
- `./test.sh` is fully green; output is equivalent to before.
- `scripts/check-deletable.sh` Python count drops from 15 to 12
  (or 13 if you keep `lexer-oracle` as `.sh`).

## Forbidden

- Editing `bootstrap/`, `kernel/`, `compiler/`, `stdlib/`.
- Adding new Python.
- Re-implementing in a language other than Bash (Evident rewrite
  is a later separate task; for now Bash is the transition layer).
- Editing any `.py` files other than the three targets (which
  you're deleting, not editing).
- Disrupting `tests/conformance/conftest.py` or anything else not
  in the target list.

## Reporting back

- Branch pushed.
- Diff summary (`git diff --stat`).
- `./test.sh` final line.
- `scripts/check-deletable.sh` Python count before / after.

Be terse.
