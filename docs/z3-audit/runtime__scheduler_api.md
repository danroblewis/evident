# runtime/src/runtime/scheduler_api.rs — Z3-replaceability
**What it does:** Provides `query_with_pinned_datatypes` and `query_with_pins_and_given` — the per-tick query entry points used by the multi-FSM scheduler to fix `state` and `last_results` as Z3 Datatype pins before solving a claim. Handles the JIT fast path, the slow-path cached solver, and falls back to `evaluate_with_extra_assertions`.
**Criticality:** critical (hot per-tick path — every FSM tick calls through here)
**Verdict:** hot-path
**Confidence:** high
**How (if replaceable):** Called by `effect_loop/scheduler.rs` on every scheduler tick for every active FSM. The file's job is to wire Datatype pins into the Z3 solver and dispatch the solve — it is the per-tick query harness, not a CSP. Running a Z3 solve to decide how to run the next Z3 solve would be circular and would add overhead with no benefit. The logic is pure orchestration (pin → JIT fast path → cached slow path → full evaluate).
**Change made:** none
