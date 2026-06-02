# Task: Build the implementation-agnostic conformance test architecture

## Goal

Today, every test in `tests/conformance/*.py` invokes the bootstrap
compiler directly. That's a problem for self-hosting: there's no
way to run the same test against both implementations and compare.

Build a new conformance structure that:

1. Defines language features as **implementation-agnostic specs**
   (an Evident source file + expected output).
2. Has a runner that compiles each spec via a swappable backend
   (`IMPL=bootstrap` today; `IMPL=selfhost` once `compiler.smt2`
   exists; `IMPL=both` to verify equivalence during the transition).
3. Migrates at least 5 representative tests from
   `tests/conformance/*.py` to the new format, proving the
   architecture handles real cases.
4. Wires the new runner into `./test.sh` as an additional phase
   (don't remove the legacy Python tests yet; they still catch
   regressions during transition).

## Why this matters

This is the strangler-fig pattern for the transition. The
conformance feature specs let us answer "for capability X, do both
compilers produce the same output?" mechanically. That's how we
know when we can flip the IMPL default from `bootstrap` to
`selfhost` and eventually delete bootstrap.

Without this scaffold, every test is bootstrap-only and there's no
way to track self-hosting progress with anything other than prose.

## Acceptance

The session is successful when ALL of these are true:

1. Directory structure exists:
   ```
   tests/conformance/features/
     README.md
     runner.sh
     001-declare-int-membership/
       source.ev
       claim.txt          # The claim name to compile
       expected/
         smt2-contains    # Lines that must appear in the .smt2 output (substring match each)
         stdout           # Optional: kernel stdout when run
         exit             # Optional: kernel exit code (default 0)
     002-...
     ...
   ```

2. `tests/conformance/features/runner.sh`:
   - Accepts `IMPL=bootstrap` (default) or `IMPL=selfhost` env.
   - For `bootstrap`: invokes `bootstrap/runtime/target/release/evident emit ... -o /tmp/out.smt2`,
     then optionally runs that via `kernel/target/release/kernel`.
   - For `selfhost`: invokes `kernel/target/release/kernel compiler.smt2 < source.ev > /tmp/out.smt2`
     (if `compiler.smt2` exists at the repo root; otherwise the
     feature is reported as "BLOCKED: no compiler.smt2").
   - For each feature directory: compiles, checks `expected/smt2-contains`
     lines each appear in /tmp/out.smt2, optionally runs the .smt2
     via kernel and checks stdout/exit.
   - For `IMPL=both`: runs both, compares both stdouts to expected
     (or to each other when no expected is given).
   - Reports `N passed / M failed / K blocked` at the end.
   - Exits 0 only if all features passed.

3. At least 5 features are migrated from `tests/conformance/*.py`.
   Pick small, focused ones — primitive memberships, simple binop,
   enum declaration, simple match, string literal. Look at the
   existing Python tests in `tests/conformance/test_*.py` for
   inspiration, but the new specs should be implementation-agnostic
   (no Python).

4. `tests/conformance/features/README.md` describes the spec format:
   - what a feature directory looks like
   - what each file means
   - how to write a new feature
   - how IMPL=both interpretation works

5. `test.sh` runs the new runner (under `IMPL=bootstrap`) as an
   additional phase, AFTER the existing conformance Python tests
   so legacy + new both run during the transition.

6. `./test.sh` passes.

7. The diff DOES NOT touch `bootstrap/`, `kernel/`, or any `.py`
   file. It MAY add to `test.sh` and the new directory.

## How to do it

1. `git checkout -b agent-conformance-features origin/freeze-and-restructure`.
2. Read 3-4 existing `tests/conformance/test_*.py` to understand
   what kinds of properties get tested. Pick 5 small ones to migrate.
3. Design the feature directory layout. Look at how tests/kernel/*.ev
   header comments declare `expected: stdout = ...` for inspiration.
4. Implement `runner.sh` with at least the `bootstrap` and
   `selfhost` impl paths. `selfhost` may be partially stubbed if
   `compiler.smt2` doesn't yet exist — just report `BLOCKED` cleanly.
5. Write 5 feature directories. Run them through `runner.sh` under
   `IMPL=bootstrap` to verify the architecture works.
6. Wire into `test.sh` as a new phase.
7. Run `./test.sh`, fix breakage, commit, push.

## Forbidden

- Editing any file under `bootstrap/`.
- Adding new `.py` files (the migration is OUT OF Python; the new
  specs are not Python).
- Modifying existing `tests/conformance/*.py` (they stay frozen and
  keep running until the migration is complete in a later task).
- Editing `kernel/`.
- Editing CLAUDE.md.

## Reporting back

Final message must include:

- Branch pushed.
- `ls tests/conformance/features/` output.
- The number of features added + their names.
- Output of `IMPL=bootstrap tests/conformance/features/runner.sh` (final summary line).
- Output of `IMPL=selfhost tests/conformance/features/runner.sh` (final summary; expected to report "no compiler.smt2" or similar).
- `./test.sh` final line.

Be terse. The coordinator can read files directly.
