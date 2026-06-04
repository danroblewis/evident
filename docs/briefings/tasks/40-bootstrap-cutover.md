# Task: bootstrap cutover — flip test.sh and delete bootstrap/

## Why

Wave 4h closed the last code-gen blocker: `test_hello` now passes
`scripts/diff-vs-bootstrap.sh --semantic` (exit 0, SEMANTIC MATCH).
The self-hosted compiler is feature-complete for the corpus we've
been driving.

CLAUDE.md's "Definition of done" lists 5 conditions; we've
mechanically reached 4. The remaining steps are the cutover:

> c. Flip `test.sh` and `scripts/evident-self` to use `kernel +
>    compiler.smt2`.
> d. Run `scripts/check-deletable.sh`. It should print "BOOTSTRAP
>    DELETABLE NOW."
> e. `rm -rf bootstrap/`. Commit. Done.

This task is that cutover. After this lands, `bootstrap/` is gone
and the project is done.

## Authorisation

This is the cutover wave. Authorised edits:

- `test.sh` — rewrite to remove phases 1-2 (bootstrap build + cargo
  test on bootstrap/runtime/) and default to `EVIDENT_SELF_VIA_SMT2=1`.
- `scripts/evident-self` — drop the bootstrap fallback; the
  self-hosted wrapper is the ONLY behaviour. (Keep the `bin` interface
  shape since many scripts call it.)
- `scripts/*.sh` — incidental fixups if their bootstrap references
  break after the flip.
- `scripts/check-deletable.sh` — only if its blockers list needs
  updating to reflect the post-cutover state.
- `docs/` — write the wave doc + update STATE.md + tick the
  DELETION-CHECKLIST.
- `compiler.smt2` — rebuild it ONCE via `scripts/build-compiler-smt2.sh`,
  commit the result. (This is the final pre-deletion build.)
- `bootstrap/` — only the FINAL `rm -rf` after everything else
  passes.
- `kernel/` — ONLY if a blocker surfaces during verification (e.g.
  the kernel needs to handle a corner case the bootstrap currently
  hides). Editing kernel/ is in-scope ONLY if cutover can't proceed
  without it; document the change in the wave doc.

Forbidden: editing `stdlib/`, `compiler/`, or `tests/` to make
cutover succeed (those are the source-of-truth; if a test fails
under the seam, it's signal the self-hosted compiler has a gap,
not signal to edit the test). No new Python anywhere.

## Required reading

1. `CLAUDE.md` — especially "Definition of done" and "The deletion
   path."
2. `docs/plans/grammar-wave4h.md` — wave 4h's verification evidence.
3. `scripts/evident-self` — the cutover seam (`EVIDENT_SELF_VIA_SMT2=1`,
   `emit_via_smt2_wrapper`).
4. `test.sh` — what phases exist; which depend on bootstrap.
5. `scripts/build-compiler-smt2.sh` — the one-time-handoff producer.
6. `scripts/check-deletable.sh` — the single source of truth.
7. `STATE.md`.

## Scope

### Item 0: pre-flight

Run, capture, do NOT proceed if these fail:

```
scripts/build-compiler-smt2.sh
ls -la compiler.smt2
bash scripts/check-deletable.sh
```

Expect `compiler.smt2` to build and `check-deletable.sh` to show
the test.sh-and-bootstrap blockers still present.

### Item 1: probe — does the seam work end-to-end?

Run the full kernel + conformance + lang phases under the seam,
WITHOUT modifying test.sh yet:

```
EVIDENT_SELF_VIA_SMT2=1 bash test.sh --kernel    2>&1 | tee /tmp/probe-kernel.log
EVIDENT_SELF_VIA_SMT2=1 bash test.sh --conformance 2>&1 | tee /tmp/probe-conformance.log
EVIDENT_SELF_VIA_SMT2=1 bash test.sh --lang      2>&1 | tee /tmp/probe-lang.log
```

This is the SLOW part — the kernel phase under the seam runs every
`evident emit` through `kernel + compiler.smt2` (~minutes per test
that needs an emit). Budget hours. Run them in parallel if your
environment supports it; otherwise serial.

**If any phase fails:**

- Capture the first failing test's exact `evident emit` error / kernel
  error / output diff against bootstrap (`scripts/diff-vs-bootstrap.sh
  --semantic <fixture> <claim>`).
- Document in `docs/plans/blocked-bootstrap-cutover.md`: which test,
  which shape, the exact diagnostic.
- STOP. Don't proceed to Item 2. The cutover is gated on green
  phases.

**If all three pass:** the self-hosted compiler is verified at the
test-suite level. Proceed to Item 2.

### Item 2: rewrite test.sh

Remove phases 1-2 (bootstrap build + cargo test on bootstrap/runtime/).
Keep the `kernel/` cargo test if you want kernel-side unit coverage
(it's not a bootstrap dependency).

New test.sh phases (suggested):

1. Build the kernel (`cd kernel && cargo build --release`).
2. (Optional) Cargo test the kernel (`cd kernel && cargo test --release`).
3. Conformance features under `IMPL=selfhost` (or whatever the
   runner exposes once bootstrap is gone).
4. Lang tests.
5. Kernel tests.

The seam (`EVIDENT_SELF_VIA_SMT2=1`) becomes the DEFAULT — either
export it inside test.sh, or rewrite `scripts/evident-self` to drop
the bootstrap branch entirely so the env var is no longer needed.
The latter is cleaner.

### Item 3: drop the bootstrap branch from scripts/evident-self

Edit `scripts/evident-self`:

- Remove the `EVIDENT` variable (the bootstrap binary path).
- Remove the env-var guard in the `bin` case; ALWAYS return the
  self-hosted wrapper.
- Keep `emit_via_smt2_wrapper` as the producing code.

After this, no script in the repo should reference
`bootstrap/runtime/target/release/evident` directly. Grep:

```
grep -rE 'bootstrap/runtime/target' scripts test.sh tests
```

If any non-bootstrap-internal file references it, fix them.

### Item 4: verify under the NEW test.sh

```
bash test.sh
```

Must be fully green, in default mode. (Under the seam by default,
no env var.) Document wall-clock — this becomes the new baseline.

### Item 5: rerun check-deletable.sh

```
bash scripts/check-deletable.sh
```

Expected: "BOOTSTRAP DELETABLE NOW" (exit 0).

If it still lists blockers, fix those before proceeding. The
blockers should be `test.sh references bootstrap` (fixed in Item 2)
and `bootstrap/ exists` (Item 6).

### Item 6: delete bootstrap/

```
rm -rf bootstrap/
```

Commit. Push.

### Item 7: final verification

```
bash test.sh
bash scripts/check-deletable.sh
```

Both should be green / exit 0.

### Item 8: post-deletion housekeeping

- Update `STATE.md` to reflect the final state.
- Update `docs/plans/DELETION-CHECKLIST.md` — tick the cutover phase.
- Write `docs/plans/cutover.md` with: wall-clock numbers, what
  changed in test.sh, the deletion commit hash, a short note on
  what comes next (the post-deletion roadmap from `docs/plans/ideas.md`
  is fair game).
- (Optional) update `CLAUDE.md` — the freeze rules section can be
  trimmed since bootstrap is gone. Don't rewrite, just remove the
  bootstrap-frozen row from the table.

### Item 9: commit the compiler.smt2 build artifact

Wave 4h reverted compiler.smt2. The post-cutover repo NEEDS it
committed (the kernel runs from it). Verify it's tracked:

```
git ls-files compiler.smt2
```

Should print `compiler.smt2`. If not, re-add (and remove from
`.gitignore` if needed).

## Acceptance

1. All three probe phases (Item 1) green under the seam.
2. test.sh rewritten; default green; bootstrap references gone.
3. `scripts/check-deletable.sh` exits 0 with "BOOTSTRAP DELETABLE NOW."
4. `rm -rf bootstrap/` committed.
5. Post-deletion `test.sh` still green.
6. Post-deletion `check-deletable.sh` still exits 0.
7. `compiler.smt2` is tracked.

## Forbidden

- Adding Python.
- Modifying `stdlib/`, `compiler/`, `tests/` to paper over cutover
  failures. (Document them and stop instead.)
- Skipping the probe in Item 1.
- Deleting bootstrap/ before Item 5 prints "BOOTSTRAP DELETABLE NOW."

## Known gotchas

- The kernel-phase probe under the seam is SLOW. Don't time out
  prematurely. Budget hours.
- `scripts/run-kernel-tests.sh` resolves evident via `evident-self bin`,
  so the seam catches it automatically. `scripts/run-lang-tests.sh`
  and `tests/conformance/features/runner.sh` should be checked the
  same way.
- If a kernel test was added recently that uses shapes the
  self-hosted compiler doesn't handle, that's a SIGNAL the compiler
  needs another wave — not a signal to skip the test. Document
  precisely.
- `.gitignore` still has `compiler.smt2` (wave 4h's revert left it
  but the file is tracked). Remove the gitignore line for clarity;
  the tracked-but-gitignored state is confusing.

## Reporting back

- Branch pushed (`agent-40-bootstrap-cutover`).
- Items 1-9 status.
- The `check-deletable.sh` final output (the headline).
- Wall-clock for the probe phases.
- The bootstrap-deletion commit hash.
- Cite docs.

This is the final wave. Be terse.
