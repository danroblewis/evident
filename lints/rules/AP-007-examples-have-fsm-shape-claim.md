# AP-007: examples-have-fsm-shape-claim

**Status:** active

**Pattern.** A file under `examples/` declares no claim with the
FSM shape — i.e., no claim has a `state, state_next ∈ <enum>`
pair AND `last_results ∈ ResultList` AND `effects ∈ EffectList`
in its parameters / body.

**Why.** Examples are runnable multi-FSM programs. A file with
only static claims (sat_*/unsat_*) is a test fixture, not a
demo, and belongs in `tests/lang_tests/` instead. The cargo
demo driver (`runtime/tests/demos.rs`) expects to run each
example via `evident effect-run`; that requires an FSM-shape
claim.

**Fix.** Add a real FSM. If the file is genuinely just static
constraints, move it to `tests/lang_tests/` and remove its
EXPECTATIONS row from `runtime/tests/demos.rs`.

**Detection.** ast — same parser walk as AP-006.

**Pattern (ast).** Load each `examples/test_*.ev`. For each top-
level `claim`, check the body's `Membership` items for the four
required vars (state pair + last_results + effects). Fail if
no claim matches.

**Scope.**
  - Apply to: `examples/test_*.ev`.
  - Do NOT apply to: `examples/COUNTEREXAMPLES.md` or any other
    non-`.ev` file.

**Exceptions.** Same `-- interactive` opt-out as AP-006.

**Examples.**
  - Hypothetical. Not a current offender.
