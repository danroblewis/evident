# AP-008: examples-registered-in-expectations

**Status:** active

**Pattern.** An `examples/test_*.ev` file exists but isn't
referenced by a row in
`runtime/tests/demos.rs::EXPECTATIONS`.

**Why.** The demo driver (`#[test] fn each_demo_runs_to_completion`
in `runtime/tests/demos.rs`) walks `EXPECTATIONS` and runs each
demo end-to-end via the binary, asserting on exit code and
stdout. A demo absent from `EXPECTATIONS` doesn't run in CI;
it can break silently. The whole "demo IS test" contract relies
on the demo driver actually running it.

**Fix.** Add a row to `EXPECTATIONS` with the demo's expected
exit code and the ordered list of stdout lines that must
appear (see existing rows for the shape — `must_lines` is a
sequence, `forbid_exact_lines` catches placeholder output).

**Detection.** ast — needs to parse `runtime/tests/demos.rs` to
extract the EXPECTATIONS table's `name:` fields. (Also
implementable as a grep but the Rust source is cleaner to walk.)

**Pattern (ast).**
  1. Find `EXPECTATIONS` `const` in `runtime/tests/demos.rs`.
  2. Extract the value of every `name:` field in the array.
  3. List `examples/test_*.ev`.
  4. Set difference: any example not in EXPECTATIONS is a
     violation, unless it has the `-- interactive` opt-out tag
     in its first 30 lines.

**Scope.**
  - Apply to: `examples/test_*.ev` × `runtime/tests/demos.rs`.

**Exceptions.** A file may opt out of EXPECTATIONS coverage by
including `-- interactive` in its first 30 lines. Used for
demos requiring real stdin (`test_14_stdin.ev`), SIGINT
(`test_15_signal.ev`), or visual verification only
(`test_16_sdl_red.ev`, `test_17_sdl_triangle.ev`). Documented
in `runtime/tests/demos.rs` as the explicit-opt-out tag.

**Examples.**
  - `test_15_signal.ev` is currently opted out (commented in
    EXPECTATIONS as "needs SIGINT").
  - `test_14_stdin.ev` HAS an EXPECTATIONS row that pipes input
    via the `stdin:` field, so it isn't opted out.
  - `test_16_sdl_red.ev`, `test_17_sdl_triangle.ev` are partial:
    sdl_red is opted out (needs display); triangle has a row
    that asserts on "done" line.
