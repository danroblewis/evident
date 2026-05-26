# runtime/src/runtime/nested.rs — Z3-replaceability
**What it does:** Implements tier-3 blocking-interpret for `run(F, init)` expressions: rewrites `RunFsm` nodes in a schema body into literal final-state values by actually running the named FSM to halt before the outer solve. Also handles load-time `lower_fsm_application` (rewrites two-arg FSM calls into `RunFsm` nodes) and `validate_run_targets`.
**Criticality:** critical (load-time + query-time, in the constraint-building pipeline)
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** This file IS part of the pipeline that produces Z3 input. The `resolve_runs` pass rewrites the AST before it is handed to the translator; `lower_fsm_application` transforms the AST at load. Both steps must complete before Z3 ever sees the schema. Replacing them with a Z3 solve would require Z3 to already be running — circular. The "run FSM to halt" interpreter (`eval_run`) also internally calls `run_nested_capturing`, which itself invokes Z3. There is no CSP here; it is the constraint-rewriting harness.
**Change made:** none
