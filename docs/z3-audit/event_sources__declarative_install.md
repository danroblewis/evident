# runtime/src/event_sources/declarative_install.rs — Z3-replaceability

**What it does:** One-shot install bridge: calls `rt.query_with_pins_and_given` to evaluate a FTI type's `install` body, decodes the resulting `Seq(InstallStep)`, dispatches the effects atomically, and queues world-field writes back to the scheduler.

**Criticality:** critical

**Verdict:** not-a-CSP

**Confidence:** high

**How (if replaceable):** The decision logic — "query the body, decode the Seq, dispatch effects, write results" — is a procedural sequence that bridges a Z3 solve output to live IO (FFI handle registration, C library init). The Z3 query inside `run_install` is already a constraint solve; what surrounds it is IO dispatch and bookkeeping. No useful constraint reformulation exists: the install steps produce OS handles that only exist after the FFI calls run.

**Change made:** none
