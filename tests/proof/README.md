# Invariant-proving examples

`scripts/prove-invariants.sh <fixture.ev> <claim> <field-const-prefix>` proves a
carried type invariant is 1-inductive over the FSM's one-tick transition (Z3
k-induction), or returns the breaking carry-state.

- `counter_guarded.ev`   — `n` bounded by a guard → invariant `0 ≤ n ≤ 10` PROVEN
  (`scripts/prove-invariants.sh tests/proof/counter_guarded.ev main c_` → step: unsat).
- `counter_unguarded.ev` — unguarded `n++` → NOT inductive, counterexample `_c_n=10`.
- real example: `scripts/prove-invariants.sh tests/compiler2_units/types/fti_buffer_carry.ev main buf_`
  finds the FtiBuffer overrun `_buf_count=2048` (unguarded append past cap).

See docs/plans/ and the memory note for the recipe + scaling (proven sub-second
even on the full driver.ev transition; sweet spot = convex few-variable invariants).
