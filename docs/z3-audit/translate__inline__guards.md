# runtime/src/translate/inline/guards.rs — Z3-replaceability
**What it does:** Three small helpers for the inline walker: `track_assert` (assert a Bool into the solver, optionally tracked for unsat-core extraction), `guard_is_satisfiable` (push/check/pop to prune dead guarded-claim expansions), and `compose_guards`/`guarded_bool` (compose an outer guard with an inner guard via `∧` / `⇒`).
**Criticality:** critical
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** These helpers directly manipulate the Z3 `Solver` object — asserting constraints and running satisfiability checks — as part of the compilation pass that builds Z3 input. They orchestrate the solver during constraint compilation; they are not a standalone CSP problem. Replacing them with a Z3 solve would be circular.
**Change made:** none
