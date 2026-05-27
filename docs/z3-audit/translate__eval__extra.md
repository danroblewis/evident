# runtime/src/translate/eval/extra.rs — Z3-replaceability
**What it does:** Three one-shot `evaluate` variants: `evaluate_with_extra_assertion` pins a single enum-typed Z3 variable before the solve (used to inject encoded `Program` values for self-hosted passes); `evaluate_with_extra_assertions` pins multiple enum-typed variables; `evaluate_with_program_and_body` additionally injects a `Seq(BodyItem)` by encoding body items directly into the array/length Z3 vars.
**Criticality:** peripheral (used by self-hosted pass paths in `runtime/reflection.rs` and `introspect.rs`, not on the normal per-tick FSM scheduler path)
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** Each function builds a Z3 solver, translates the schema body into it, asserts extra values, calls `solver.check()`, and extracts the model. These are solve-driver entry points — they ARE the code that invokes Z3. They cannot be replaced by a Z3 constraint solve without infinite regress.
**Change made:** none
