# Task: Python→Bash migration, wave 1

## Why

`scripts/check-deletable.sh` reports 17 Python files under `scripts/`
and `tests/conformance/`. Each one is on the deletion path. Wave 1
targets the two simplest, most-independent scripts to prove the
migration pattern.

## Required reading

1. `CLAUDE.md` (freeze rules — Python is FROZEN but DELETE-when-replaced is allowed).
2. `docs/plans/DELETION-CHECKLIST.md` (where this fits).
3. `scripts/check-deletable.sh` (so you understand which files
   counted).

## Targets (this wave)

1. `scripts/runtime-size.py` — reports LOC for the bootstrap Rust
   runtime + Evident stdlib/passes. Pure read-and-count.
2. `scripts/strip-comments.py` — strips comments from Rust source
   for dump-codebase.sh.

Both are small, self-contained, and not called from `test.sh`
critical path.

## What you're producing

For each target:

1. Write `scripts/<basename>.sh` (with `# TODO: rewrite in Evident`
   header per the freeze rules for transition-only Bash).
2. The `.sh` must produce *identical or equivalent* output to the
   `.py`. For `runtime-size.py`, "equivalent" means: same headers,
   same LOC numbers (or numbers that match `wc -l` directly).
   For `strip-comments.py`, "equivalent" means: when piped through
   dump-codebase.sh, the result is the same as before.
3. Update any caller (`scripts/dump-codebase.sh` calls strip-comments;
   nothing calls runtime-size externally) to point at the `.sh`.
4. `git rm` the `.py`.
5. `./test.sh` stays fully green.

## Acceptance

- 2 `.py` files removed (`runtime-size.py`, `strip-comments.py`).
- 2 `.sh` files added (with TODO header).
- 1 caller updated (`dump-codebase.sh` — check it for strip-comments
  references).
- `./test.sh` green.
- `scripts/check-deletable.sh` Python count drops from 17 to 15.

## Forbidden

- Editing `bootstrap/`, `kernel/`, `compiler/`, `stdlib/`.
- Editing other `.py` files. Only delete `runtime-size.py` and
  `strip-comments.py`.
- Adding new `.py` anywhere.
- Re-implementing in Evident now (transition wants Bash for these
  short scripts; the Evident rewrite is a later, separate task).

## Reporting back

- Branch pushed.
- Output of `scripts/runtime-size.sh` (one screenful is fine).
- `./test.sh` final line.
- `scripts/check-deletable.sh` Python count before / after.

Be terse.
