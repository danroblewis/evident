# Things parked for porting to Rust

These were the Python-runtime-era test suites and other artifacts.
They reference the Python AST shape (`runtime/src/`, `parser/src/`)
that was deleted in the cleanup. Kept here as a checklist for what
to port, not as live tests.

## conformance/

Pytest suite that exercised the Python interpreter end-to-end —
language features, claim composition, error paths, the legacy
adventure / TCP / SDL programs. The Rust integration suite under
`runtime/tests/` has overlapping coverage in places but not
all of it. Worth scanning before adding new Rust tests, to find
behaviors the old suite asserted that the Rust suite doesn't yet.

To port a file:
  1. Read the Python test, identify what behavior it asserts.
  2. If the behavior still applies to the current language /
     runtime, write an equivalent Rust integration test (under
     `runtime/tests/` — see `tests/multi_fsm.rs` and
     `tests/demos.rs` for shape).
  3. Delete the Python file once the Rust test exists and passes.

When this directory is empty, delete it.
