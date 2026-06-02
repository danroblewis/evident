# Task: Implement the kernel fix — one solver, push/pop per tick

## Authorisation

**This task is one of the rare exceptions to the `kernel/` freeze.**
The user has explicitly approved editing `kernel/src/tick.rs` to
implement the fix specified at
`docs/plans/kernel-fix-incremental-solving.md`. The user's quote:
*"Approve, that is what the previous code did and it's the most
sensible solution. I thought we were doing this already."*

Your edits are limited to what the proposal specifies. Do NOT take
the approval as license to do other kernel work, refactors, or
"cleanups." When in doubt, read the proposal again and do less.

## Required reading

1. `CLAUDE.md` (freeze rules + Z3 lifecycle expectations).
2. `docs/plans/architecture-invariants.md` (the user-confirmed
   rules this fix implements).
3. `docs/plans/audit-kernel-z3-lifecycle.md` (the audit that
   identified the gap, with file:line citations).
4. `docs/plans/kernel-fix-incremental-solving.md` (the
   proposal — your spec).
5. `kernel/src/tick.rs` (the file you're editing).
6. `legacy-python/docs/runtime-architecture.md` (background on the
   minimal-runtime model).

Cite at least #3 and #4 in your report.

## What you're changing

In `kernel/src/tick.rs`:

1. **Move solver creation out of the tick loop.** Currently a
   fresh `Z3_solver` is created and the body re-parsed every tick
   (per audit, lines 62-111). After the fix:
   - `Solver::new` runs ONCE before the tick loop.
   - `Solver::from_string(body)` (or equivalent) runs ONCE,
     populating the solver with the program's asserted constraints.
   - The solver is reused for every tick.

2. **Use `push` / `pop` for tick-local equalities.** Per tick:
   - `solver.push()`.
   - Assert the tick-local pinning equalities: `is_first_tick = …`,
     `_<state> = <previous model's value>`, `last_results = …`.
   - `solver.check()` for SAT.
   - Read the model (effect dispatch + state extraction stay as-is).
   - `solver.pop()`.

3. **NOTHING ELSE.** Effect dispatch logic, manifest header
   parsing, halt rules, state-field extraction, error reporting:
   all unchanged.

## Acceptance

All of:

1. `kernel/src/tick.rs` modified per the proposal. No other Rust
   files modified.
2. `./test.sh` is fully green:
   - Build + cargo test pass.
   - Conformance phases pass.
   - lang_tests pass.
   - All 61 kernel tests pass.
   - The 6 conformance features pass.
3. The proposal asked for a before/after measurement. Provide a
   simple wall-clock comparison on a representative fixture:
   - Pick a fixture that runs multiple ticks (e.g.
     `tests/kernel/test_consolidated_lexer.ev` runs ~13 ticks).
   - Run it on `main` (without your fix) and capture wall-clock.
   - Run it on your branch (with your fix) and capture wall-clock.
   - Report both numbers in your final message.
4. Diff is limited to:
   - `kernel/src/tick.rs` (modified)
   - Possibly `docs/plans/kernel-fix-incremental-solving.md`
     (marked LANDED).
   - Possibly `docs/plans/architecture-invariants.md` (the
     "VIOLATES … fix proposal at …" sentence updated to "FIX
     LANDED at commit …").
5. `scripts/check-deletable.sh` output unchanged (this is a kernel
   change, not a deletion-path change — the blocker count stays
   the same).

## Forbidden

- Editing any kernel file OTHER than `kernel/src/tick.rs`.
- Editing `bootstrap/`, `compiler/`, `stdlib/`, or anything outside
  `kernel/` (beyond the documentation updates in §4 above).
- Adding new Rust crate dependencies.
- "Cleanups" or refactors. The proposal is the spec.
- Adding any `.simplify()` calls anywhere.
- Removing the manifest header parsing, effect dispatch, or halt
  rules.

## Reporting back

Final message (terse):

- Branch pushed (`agent-06-kernel-fix-incremental-solving` or
  whatever).
- Before/after wall-clock on the chosen fixture (two numbers).
- Diff stat: `git diff --stat HEAD~1` (only `tick.rs` should appear).
- `./test.sh` final line.
- `scripts/check-deletable.sh` blocker count after your change
  (unchanged from baseline).
- Cite docs/plans/audit-kernel-z3-lifecycle.md and
  docs/plans/kernel-fix-incremental-solving.md.

Do NOT paste the diff itself; the coordinator reads
`git show agent-06-kernel-fix-incremental-solving`.

## If you get stuck

Most likely blockers:

1. **The z3-rs crate's `Solver::from_string` may not exist or may
   behave differently than expected.** Look at how the current
   code parses the body (`tick.rs:62-111`) and use the same
   primitives, but call them once at startup instead of per tick.

2. **Sort/handle threading across the `push`/`pop` boundary.**
   Make sure the model read happens between `check()` and `pop()`,
   and that the constants you `mk_const` for state-pinning are
   reused (not freshly allocated) per tick.

3. **The body must be re-parsable as-is.** If the current parse
   logic does anything fancy (mid-parse mutations of the manifest,
   say), preserve that exactly — just hoist it to startup.

If any blocker stops you, write
`docs/plans/blocked-kernel-fix.md` describing what you tried.
Do NOT push a half-fix to the kernel.
