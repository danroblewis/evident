# Task: final cutover + bootstrap deletion (wave 4n)

## Prerequisites — DO NOT START THIS WAVE UNLESS

1. `EVIDENT_SELF_VIA_SMT2=1 bash test.sh --lang` is GREEN.
   (Wave 4l closed Wall-2; wave 4m closed Wall-1.)
2. `EVIDENT_SELF_VIA_SMT2=1 bash test.sh --kernel` is GREEN.
3. `EVIDENT_SELF_VIA_SMT2=1 bash test.sh --conformance` is GREEN.

Verify ALL THREE before doing anything irreversible. If any is
red, the cutover is NOT ready — file a blocker and stop.

## Why

This is the closing wave. After wave 4l and 4m, the self-hosted
toolchain (`kernel + compiler.smt2` + `kernel + sample.smt2`) is
functionally complete and fast enough. The only remaining steps
are mechanical:

a. Rewrite `test.sh` to drop bootstrap phases.
b. Drop bootstrap branch from `scripts/evident-self`.
c. `scripts/check-deletable.sh` → "BOOTSTRAP DELETABLE NOW".
d. `rm -rf bootstrap/`.
e. Final verification.

CLAUDE.md "Definition of done" is met when all 5 conditions hold.
This wave makes them hold.

## Authorisation

Edit:
- `test.sh` — rewrite phases.
- `scripts/evident-self` — drop bootstrap branch.
- `scripts/*.sh` — any other bootstrap references.
- `.gitignore` — remove the `compiler.smt2` entry (kept for clarity
  since the file is tracked).
- `CLAUDE.md` — trim the freeze-rules table (remove the
  bootstrap row).
- `STATE.md` — final state.
- `docs/` — wave doc.
- `bootstrap/` — ONLY the final `rm -rf`.

Forbidden: editing `stdlib/`, `compiler/`, `kernel/`, `tests/`
to make the cutover succeed.

## Required reading

1. `CLAUDE.md` — "Definition of done", "The deletion path".
2. `STATE.md` — current state.
3. `docs/plans/wave-4l-…md`, `docs/plans/wave-4m-…md` — what
   the prerequisites established.
4. `scripts/check-deletable.sh` — the single source of truth.
5. `test.sh` — what phases exist; which depend on bootstrap.

## Scope

### Item 1: re-verify prerequisites

Run all three phases under the seam. Capture wall-clock for each.
Confirm GREEN. If any fail, STOP — this wave is not yet ready.

### Item 2: rewrite test.sh

Drop:
- Phase 1 (bootstrap runtime build).
- Phase 2 (cargo test bootstrap/runtime/).

Keep:
- Build the kernel.
- Cargo test the kernel.
- Conformance features.
- Lang tests.
- Kernel tests.

Set `EVIDENT_SELF_VIA_SMT2=1` as the DEFAULT inside test.sh, OR
rewrite `scripts/evident-self` to drop the bootstrap branch
entirely (then the env var is no-op). Latter is cleaner — do it
that way and skip the export.

### Item 3: drop bootstrap branch from scripts/evident-self

Per Item 2's choice: in `scripts/evident-self`, remove:
- The `EVIDENT` variable (bootstrap path).
- The env-var guard in the `bin` case.
- The bootstrap fallback path.

`bin` always returns the self-hosted wrapper. The env var
`EVIDENT_SELF_VIA_SMT2` becomes vestigial; you can delete it from
the documentation block in the same script.

Grep for stale bootstrap references:

```
grep -rE 'bootstrap/runtime/target' scripts test.sh tests
```

Anything that pops up gets fixed (or proven to be doc-only and
irrelevant).

### Item 4: verify under the new test.sh

```
time bash test.sh
```

Must be GREEN. Document wall-clock.

### Item 5: scripts/check-deletable.sh

```
bash scripts/check-deletable.sh
```

Expected output: `BOOTSTRAP DELETABLE NOW` (exit 0).

If it still lists blockers, fix the script if it's a script bug,
or fix the underlying state. The script's blockers list is
authoritative — don't ship around it.

### Item 6: rm -rf bootstrap

```
rm -rf bootstrap/
```

Commit the deletion as its own commit:

```
git commit -m "rm -rf bootstrap/ — self-hosting complete"
```

### Item 7: post-deletion verification

```
bash test.sh
bash scripts/check-deletable.sh
```

Both green / exit 0. Document.

### Item 8: housekeeping

- Update `STATE.md` to reflect FINAL state ("DONE").
- Tick `docs/plans/DELETION-CHECKLIST.md`.
- Update `CLAUDE.md` — remove the `bootstrap/ — FROZEN` row from
  the freeze-rules table. Adjust prose elsewhere if it references
  bootstrap as still-present. (Don't rewrite the whole file —
  surgical edits only.)
- Remove `.gitignore` entry for `compiler.smt2` (it's tracked
  and should not be confusable).

### Item 9: wave doc

Write `docs/plans/cutover-done.md` with:
- Wall-clock numbers (before/after for the key phases).
- The deletion commit hash.
- A brief "what comes next" pointing to `docs/plans/ideas.md`
  (the self-composing transition functions idea, the
  Seq-as-interface design, the FTI-around-C-library work).

## Acceptance

1. Items 1-9 complete.
2. `bash scripts/check-deletable.sh` exits 0 with `BOOTSTRAP
   DELETABLE NOW`.
3. `bootstrap/` does not exist.
4. `bash test.sh` is GREEN, no bootstrap references anywhere.
5. CLAUDE.md and STATE.md reflect the final state.

## Forbidden

- Editing `stdlib/`, `compiler/`, `kernel/`, `tests/` to make
  cutover succeed (those are source-of-truth; if anything fails,
  the prerequisite waves weren't actually done — go back).
- Adding Python.
- Deleting bootstrap before Item 5 prints "BOOTSTRAP DELETABLE
  NOW".

## Known gotchas

- Bootstrap may have references in unexpected places. `grep -r
  bootstrap` after Item 3 will catch them.
- `compiler.smt2` MUST stay tracked after deletion (the kernel
  needs it). Verify with `git ls-files compiler.smt2` after the
  deletion commit.
- The repo's CI / any pre-commit hooks that depend on bootstrap
  also need updating (or removing). Check
  `.git/hooks/`, `lints/`, etc.

## Reporting back

This is the FINAL wave. Report:

- Deletion commit hash.
- `bash scripts/check-deletable.sh` final output.
- `bash test.sh` final wall-clock.
- Branch (`agent-45-final-cutover`).
- Cite docs.

When this lands and is merged: the project is DONE per CLAUDE.md.
