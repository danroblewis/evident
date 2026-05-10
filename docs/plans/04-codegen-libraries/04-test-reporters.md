# Phase 4.4: Test reporters → stdlib/testing/reporters/

## Goal

Replace TAP / JUnit / JSON formatter functions in
`runtime/src/commands/test.rs` (~400 lines) with
`stdlib/testing/reporters/` Evident libraries.

## Prereqs

- Phase 3 done (recursive walks for nested test results).

## What to build

`stdlib/testing/reporters/tap.ev`,
`stdlib/testing/reporters/junit.ev`,
`stdlib/testing/reporters/json.ev` — each takes a Vec<TestResult>
(via the AST encoder) and produces a String.

The Rust test runner stays as the orchestration layer (loading
files, running queries) but emits a `TestResult` value that the
Evident reporter formats. The `--format=tap` flag in CLI just
selects which reporter to invoke.

## Files touched

- `runtime/src/commands/test.rs` — delete the formatter functions,
  replace with calls to the Evident reporters.
- `stdlib/testing/reporters/*.ev` (new)
- `runtime/src/ast.rs` — possibly a TestResult enum if not
  already part of the AST.

## Acceptance

- [ ] All three formats produce byte-identical output to the current
      Rust formatters
- [ ] LOC: -400 Rust, +~300 Evident

## Notes

Byte-identical output is the safety property. Diff-test thoroughly
on the existing test suite.
