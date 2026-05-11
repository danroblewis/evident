# AP-006: examples-have-sat-or-unsat-claims

**Status:** active

**Pattern.** A file under `examples/` declares no `claim sat_*`
or `claim unsat_*`. The file is an example program but not a
test.

**Why.** The contract for `examples/test_NN_<name>.ev` is that
each file is BOTH a worked example AND an integration test.
Inline `sat_*`/`unsat_*` claims test invariants of the program's
FSM at the constraint level. A file with no test claims is in
the wrong directory — it should live elsewhere or grow tests.

**Fix.** Add at least one `claim sat_*` or `claim unsat_*` that
asserts a real property of the program's FSM. Pin the relevant
state/inputs and assert what should happen. (See existing demos
for examples — `test_01_hello.ev` is the minimum viable shape.)

**Detection.** ast — needs the runtime's parser to enumerate
claim names. (Could also be done as a grep for `claim sat_` /
`claim unsat_` but a simple grep miscounts comments and
multi-line claims; better to walk the parsed file.)

**Pattern (ast).** Load each `examples/test_*.ev`; count the
claims whose name starts with `sat_` or `unsat_`. Fail if the
count is zero.

**Scope.**
  - Apply to: `examples/test_*.ev`.
  - Do NOT apply to: any other path. `tests/lang_tests/` files
    are not subject to this rule (they're regression fixtures
    for the Rust harness; the harness itself is the test).

**Exceptions.** A file may opt out by including the exact line
`-- interactive` somewhere in its first 30 lines (same opt-out
mechanism as AP-008's EXPECTATIONS-row rule). Used for demos
that need real stdin or SIGINT to be meaningful (e.g.,
`test_15_signal.ev`).

**Examples.**
  - Hypothetical. Not a current offender — every example today
    has at least one `sat_*` claim.
