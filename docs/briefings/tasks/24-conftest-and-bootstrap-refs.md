# Task: Migrate conftest.py + remove test.sh bootstrap binary refs

## Why

This is task #4 in the queue plan. Two of the five
`scripts/check-deletable.sh` blocker categories shrink with this
work:

- `1 Python file remains` → 0
- `11 files still reference bootstrap/runtime/target` → fewer (the
  test runner scripts that go through `scripts/evident-self`
  instead of bootstrap will drop their bootstrap refs)

Even before `compiler.smt2` exists, we can route every consumer
through `scripts/evident-self`, which today wraps bootstrap but
becomes the seam where the bootstrap→kernel switch happens
later. The point of this task is to *localise* all bootstrap-binary
invocations into a single script.

## Authorisation

You may edit `scripts/*.sh`, `test.sh`, `tests/conformance/*`
(including deleting Python), and `docs/`. No `kernel/` or
`bootstrap/` edits. Sessions 17/20 already added
`tests/kernel/test_compiler_driver_mvp.ev` and
`test_compiler_driver_readfile.ev`; don't disrupt those.

## Required reading

1. `CLAUDE.md` (freeze table).
2. `STATE.md` (current blocker count — the diff after your work
   will be a real change here).
3. `scripts/check-deletable.sh` (so you understand what's being
   counted).
4. `scripts/evident-self` (the seam where bootstrap-binary use
   should be centralised).
5. `tests/conformance/conftest.py` (the last `.py`).
6. `test.sh` (multiple bootstrap invocations).
7. The shell scripts in `scripts/` that match
   `bootstrap/runtime/target` per `check-deletable.sh`'s output:
   `bench-demo.sh`, `bench-selfhosted.sh`, `diff-test-selfhosted.sh`,
   `run-kernel-tests.sh`, `run-lang-tests.sh`.

Cite #3 and #4 in your report.

## What you're producing

### Step 1: Reroute every shell consumer through `scripts/evident-self`

Today, multiple scripts call `bootstrap/runtime/target/release/evident`
directly. Change them to call `scripts/evident-self` instead, which
already wraps bootstrap and gives us a single point of control.

- `test.sh` — Phases 4 and 5 invoke bootstrap. Change them.
- `scripts/run-kernel-tests.sh` — same.
- `scripts/run-lang-tests.sh` — same.
- `scripts/bench-demo.sh`, `bench-selfhosted.sh`,
  `diff-test-selfhosted.sh` — same.

The `evident-self` script currently has a one-line fallback to
bootstrap; keep that. When `compiler.smt2` exists later, only
that one line changes.

### Step 2: Migrate `tests/conformance/conftest.py` to Bash

`conftest.py` is the last Python file under scripts/tests. What
does it do today? Probably configures pytest fixtures that
invoke bootstrap, parses results, etc. Read it carefully.

The new conformance runner (`tests/conformance/features/runner.sh`)
already runs feature specs. If `conftest.py`'s only role is the
pytest plumbing for the migrated tests (which all moved to
`features/` in wave 3), it can be deleted.

If it does anything else — pretty-printing, error message
formatting, perf measurement — port that to a Bash equivalent
under `scripts/`.

After this step: NO `.py` files anywhere under `scripts/` or
`tests/`.

### Step 3: Update test.sh phase descriptions

`test.sh` Phase 3 is "pytest tests/conformance/". After conftest.py
goes, there are no pytest tests to run. Replace Phase 3 with:
`tests/conformance/features/runner.sh` — which is what really
covers conformance now.

### Step 4: Verify the deletion-path counts moved

After your work:
- `scripts/check-deletable.sh` Python count: should be 0.
- `scripts/check-deletable.sh` bootstrap-binary-ref count:
  should be small (only `scripts/evident-self` and `STATE.md`
  itself).

Capture the before/after `check-deletable.sh` output in your
report.

## Acceptance

1. `find scripts tests -name '*.py'` returns nothing.
2. Every `bootstrap/runtime/target` reference outside
   `scripts/evident-self` is gone (the script's grep exclusions
   should be updated if needed).
3. `./test.sh` is fully green — including conformance, which now
   runs `tests/conformance/features/runner.sh` instead of pytest.
4. `scripts/check-deletable.sh` Python count drops to 0; bootstrap
   refs drop substantially.
5. `STATE.md` regenerated with the new counts.
6. Diff scoped to `test.sh`, `scripts/*.sh`,
   `tests/conformance/conftest.py` (delete), STATE.md.

## Forbidden

- Editing `bootstrap/`, `kernel/`, `compiler/`, `stdlib/`.
- Adding new Python (you're DELETING the last one).
- Disrupting the existing `tests/conformance/features/`
  fixtures (they're spec, not implementation).
- Changing `scripts/evident-self`'s contract — it stays the seam
  for the eventual bootstrap-to-kernel switch.

## Reporting back

- Branch pushed.
- Before/after `scripts/check-deletable.sh` Python count and
  bootstrap-ref count.
- List of files modified.
- `./test.sh` final line.
- Cite docs.

Be terse. The coordinator reads files.
