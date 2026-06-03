# Task: Bootstrap-cutover toolkit

## Why

Wave 3 (in flight) brings `compiler.ev` to plausible
self-compilation. The moment it lands, we want to immediately:

1. Build `compiler.smt2` (the one-time bootstrap → self-hosted handoff).
2. Have a way to switch `scripts/evident-self bin` to use kernel +
   compiler.smt2 instead of the bootstrap binary.
3. Run an equivalence check: for every `.ev` source, does
   `kernel + compiler.smt2` produce SMT-LIB equivalent to bootstrap?

This task builds the toolkit so all three are ready. None of it
depends on wave 3 being done; it's pure Bash + scripts.

## Authorisation

You may add scripts under `scripts/`, add a feature test under
`tests/conformance/features/`, and edit `scripts/evident-self`
(carefully). No `bootstrap/`, `kernel/`, `compiler/`, `stdlib/`,
or Python.

## Required reading

1. `CLAUDE.md`.
2. `STATE.md` — the 3 remaining blockers.
3. `docs/plans/DELETION-CHECKLIST.md` — Phase 5 + 6.
4. `scripts/evident-self` — the seam you'll extend.
5. `scripts/flatten-evident.sh` — the flatten preprocessor (task #28).
6. `tests/kernel/test_flatten_compiler.sh` — the flatten test
   pattern.
7. `bootstrap/runtime/src/main.rs` — to understand the `emit`
   subcommand the toolkit will compare against.

Cite #4, #5, and #6 in your report.

## What you're producing

### Script 1: `scripts/build-compiler-smt2.sh`

The one-time build of the self-hosted compiler. Does:

```
1. scripts/flatten-evident.sh compiler/compiler.ev > /tmp/compiler-flat.ev
2. bootstrap/runtime/target/release/evident emit /tmp/compiler-flat.ev main -o compiler.smt2
3. Verify compiler.smt2 is non-empty and starts with `;; manifest:`
4. Print the resulting size + a summary.
```

Optional `--check-only` flag that does steps 1+2 but writes to
`/tmp/compiler-check.smt2` so the user can preview.

Errors:
- If `compiler/compiler.ev` doesn't exist or flatten fails → exit 1
  with clear message.
- If bootstrap emit errors → propagate the error.
- Atomic: write to `compiler.smt2.tmp` then `mv` so we never have
  a half-written `compiler.smt2`.

`# TODO: rewrite in Evident` header.

### Script 2: `scripts/diff-vs-bootstrap.sh <source.ev> <claim>`

Compares `kernel + compiler.smt2` output vs `bootstrap emit`
output for one `.ev` file:

```
1. bootstrap/runtime/target/release/evident emit <source> <claim> -o /tmp/orig.smt2
2. scripts/flatten-evident.sh <source> > /tmp/flat.ev
3. kernel/target/release/kernel compiler.smt2 < /tmp/flat.ev > /tmp/self.smt2
4. diff /tmp/orig.smt2 /tmp/self.smt2
5. Exit 0 if identical (or whitespace-equivalent), 1 otherwise.
```

If `compiler.smt2` doesn't exist at the repo root, exit 0 with a
clear "SKIPPED: compiler.smt2 not built yet" message (NOT a
failure — we don't want to break test.sh before the cutover).

If kernel/target/release/kernel doesn't exist either, exit 0 with
SKIPPED.

`# TODO: rewrite in Evident` header.

### Extension to `scripts/evident-self`

Currently `scripts/evident-self bin` returns the bootstrap binary
path. Extend with an environment variable: `EVIDENT_SELF_VIA_SMT2=1`.

When set AND `compiler.smt2` exists:
- `evident-self bin` returns a wrapper path that, when invoked,
  runs `flatten input | kernel compiler.smt2` instead of bootstrap.
- Write the wrapper to `/tmp/evident-self-via-smt2-$$.sh` (or
  similar — use the existing pattern). Mark executable.

When unset (default) OR `compiler.smt2` doesn't exist:
- Behaves exactly as today (returns the bootstrap binary path).

This means the cutover is just `export EVIDENT_SELF_VIA_SMT2=1`
once compiler.smt2 exists + tests pass. Reverting is `unset`.

### Conformance feature: `tests/conformance/features/200-self-vs-bootstrap-diff/`

A spec-format feature that:
1. If `compiler.smt2` doesn't exist → reports BLOCKED (which the
   runner already handles).
2. Otherwise: runs `diff-vs-bootstrap.sh` on at least 3 simple
   fixtures from `tests/kernel/` (the MVP, multi_member, arith
   ones — they're all known to work in compiler.ev's current
   grammar).
3. Passes if all diffs are clean.

Use number `200-` so it sorts after the language-feature ones
(001-199 range).

## Acceptance

1. The 3 scripts exist, executable, with TODO-rewrite-in-Evident
   header.
2. `scripts/build-compiler-smt2.sh` runs end-to-end TODAY and
   produces a valid `compiler.smt2` (bootstrap can compile what
   `compiler.ev` is so far — even though wave 3 isn't done yet,
   the existing compiler.ev compiles cleanly).
3. `scripts/diff-vs-bootstrap.sh` exits 0 (SKIPPED) when
   `compiler.smt2` doesn't exist.
4. `EVIDENT_SELF_VIA_SMT2=1 scripts/evident-self bin` returns
   a working wrapper when `compiler.smt2` exists; otherwise
   returns the bootstrap path.
5. `./test.sh` is fully green BOTH:
   - With `EVIDENT_SELF_VIA_SMT2` unset (current behavior).
   - With `EVIDENT_SELF_VIA_SMT2=1` after running
     `scripts/build-compiler-smt2.sh` (current grammar is wave 1+2;
     equivalence will hold ONLY for inputs in that subset — that's
     fine for proof of concept).
6. The new conformance feature passes when `compiler.smt2`
   exists OR cleanly reports BLOCKED when it doesn't.
7. **You may write `compiler.smt2` to the repo root as part of
   testing your scripts BUT delete it before commit** — we don't
   want to commit a `compiler.smt2` built from wave-1+2 grammar
   when wave 3 is about to land.

## Forbidden

- Editing `bootstrap/`, `kernel/`, `compiler/`, `stdlib/`.
- Adding Python.
- Committing `compiler.smt2` itself (just the tooling that builds it).
- Editing `test.sh` (the cutover is a separate later task).

## Reporting back

- Branch pushed (`agent-30-bootstrap-cutover-toolkit`).
- Output of running `scripts/build-compiler-smt2.sh` to confirm
  it works.
- Size of the produced `compiler.smt2` (this becomes the size we
  commit when wave 3 lands).
- Output of `EVIDENT_SELF_VIA_SMT2=1 scripts/evident-self bin`
  with compiler.smt2 present (and after deleting it: confirm
  reverts to bootstrap path).
- Cite docs.

Be terse. Don't paste full script source — coordinator reads
files.
