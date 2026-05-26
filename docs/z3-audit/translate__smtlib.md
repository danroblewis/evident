# runtime/src/translate/smtlib.rs — Z3-replaceability
**What it does:** Prototype Evident→SMT-LIB text emitter for a QF scalar subset (Int/Bool/Real/String, arithmetic, comparisons, set-range membership, ternary). Hands generated text to `Z3_solver_from_string`; solves; extracts bindings. Gated behind `EVIDENT_SMTLIB=1`; not on any default translate/query path.
**Criticality:** peripheral
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** This IS the compile-target emitter — it produces SMT-LIB that is then fed to Z3. Replacing it with a Z3 solve would be circular (you need to emit SMT-LIB to run Z3). Per the session notes (SMT-LIB prototype result): the emit is ~1µs; the cost is Z3 parse+Context creation. Additive prototype, not on the hot path. Nothing to replace.
**Change made:** none
