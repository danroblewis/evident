# runtime/src/translate/eval/solver.rs — Z3-replaceability
**What it does:** Solver-construction helpers shared by all `evaluate*` entry points: `make_tuned_solver` builds a Z3 tactic chain (default `solve-eqs → smt`) from `EVIDENT_TACTICS` env var; `populate_enum_variants` pre-loads enum constructor/value entries into the env; `declare_and_assert` declares a Z3 variable of the given type and immediately asserts its type invariants; f64↔Z3-Real conversion utilities.
**Criticality:** critical (called at the start of every solve path via every `evaluate*` variant)
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** This file configures and constructs the Z3 solver object. `make_tuned_solver` creates a `z3::Solver` by composing Z3 tactics via the Z3 C API. `declare_and_assert` populates the Z3 environment by calling `declare_var` and asserting type invariants into a solver. None of this is an algorithm that can be expressed as a constraint — it is the infrastructure that makes Z3 solves possible.
**Change made:** none
