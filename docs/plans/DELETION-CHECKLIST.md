# Deletion checklist

The project is done when `scripts/check-deletable.sh` exits 0 and
`rm -rf bootstrap/` has been committed. Every item below is a
runnable acceptance: it either passes or it doesn't; no prose
judgement.

The checklist is short on purpose. Adding items because they "feel
important" is the failure mode that previously turned this into a
phase-based roadmap. Keep it concrete.

## Phase 0 — make the goal visible (DONE)

- [x] `CLAUDE.md` states the architectural shape and the deletion
      goal up front.
- [x] `scripts/check-deletable.sh` exists and exits 1 with the
      current blocker count.
- [x] `STATE.md` shows the current blocker output.
- [x] `bootstrap/READ-ME-FIRST.md` says "this exists to be deleted."
- [x] `docs/briefings/foundation.md` exists for subordinate sessions.

## Phase 1 — implementation-agnostic conformance suite

- [ ] `tests/conformance/features/` directory exists with a
      defined feature-spec format.
- [ ] `tests/conformance/features/runner.sh` runs feature tests
      under `IMPL=bootstrap` (today's path) and is structured to
      accept `IMPL=selfhost` and `IMPL=both` later.
- [ ] At least 5 features migrated from `tests/conformance/*.py`
      to the new format and passing under `IMPL=bootstrap`.

Acceptance: `IMPL=bootstrap tests/conformance/features/runner.sh`
exits 0 with N features run.

## Phase 2 — restructure stdlib/ and start compiler/

- [ ] `compiler/` directory exists at the repo root.
- [ ] `compiler/lexer.ev`, `compiler/parser.ev`, `compiler/translate.ev`
      (and supporting per-pass files) live there. Imports updated
      everywhere.
- [ ] `compiler/README.md` describes what each file is, what it
      replaces in `bootstrap/runtime/src/`, and what's missing.
- [ ] `stdlib/` contains only stable library code (kernel.ev,
      combinatorics.ev, toposort.ev, ast.ev if applicable).
- [ ] Every WIP file in `compiler/` has a top-line comment:
      `-- WIP: replaces bootstrap/runtime/src/<file>. STATUS: <one line>.`

Acceptance: `./test.sh` green; `find compiler -name '*.ev' | xargs grep -l 'WIP: replaces'`
returns every file.

## Phase 3 — the driver

- [ ] `compiler/compiler.ev` exists. Single Evident program that
      reads a `.ev` file via `ReadFile`, lexes via `compiler/lexer.ev`,
      parses via `compiler/parser.ev`, translates via
      `compiler/translate*.ev`, emits `.smt2` to stdout.
- [ ] Driver compiles successfully via bootstrap:
      `bootstrap/runtime/target/release/evident emit compiler/compiler.ev main -o compiler.smt2`.
- [ ] `kernel/target/release/kernel compiler.smt2 < some.ev`
      emits SMT-LIB for `some.ev`.

Acceptance: one round-trip — pick the simplest feature test from
Phase 1, feed its `source.ev` to the kernel-driven compiler, get
its `.smt2`, run that `.smt2` on the kernel, observe the expected
output.

## Phase 4 — feature equivalence

- [ ] Every feature in `tests/conformance/features/` passes under
      `IMPL=both` (bootstrap and self-hosted produce semantically
      equivalent output).
- [ ] `IMPL=selfhost` is the default in `tests/conformance/features/runner.sh`.

Acceptance: `IMPL=both tests/conformance/features/runner.sh` exits 0.

## Phase 5 — sever bootstrap

- [ ] `test.sh` no longer cd's into `bootstrap/` or invokes
      `bootstrap/runtime/target/release/evident`.
- [ ] `scripts/evident-self` and other helper scripts use
      `kernel + compiler.smt2`.
- [ ] All Python files under `tests/` and `scripts/` are removed
      (or replaced; new conformance is in `tests/conformance/features/`,
      new helper scripts are `.sh`).
- [ ] `compiler.smt2` is committed at the repo root.

Acceptance: `scripts/check-deletable.sh` exits 0 with
"BOOTSTRAP DELETABLE NOW".

## Phase 6 — delete

- [ ] `rm -rf bootstrap/` committed.
- [ ] `.cargo/config.toml` removed (the bootstrap build linker
      wrapper is gone).
- [ ] `CLAUDE.md` updated to reflect the project as done; the
      freeze section becomes the final-state declaration.

Acceptance: `ls bootstrap` says "No such file or directory" and
`./test.sh` is green. The project is complete.

---

## How to claim progress on this checklist

You don't tick boxes from a written-up status. You tick them by
producing the runnable acceptance and demonstrating it. If you
cannot produce the acceptance, the item is not done, regardless of
how much code you wrote.

A subordinate session reports back by including the relevant
`bash` invocation and its actual output. The coordinator reviews
that output before checking a box.
