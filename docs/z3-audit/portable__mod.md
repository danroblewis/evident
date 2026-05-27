# runtime/src/portable/mod.rs — Z3-replaceability
**What it does:** The shared infrastructure for all self-hosted runtime passes. Provides `EvidentRunner` (a per-thread cached engine that loads a `.ev` pass file, drives it as an FSM, and returns `Value` results), the `cached_runner!` / `guarded_runner!` macros (thread-local build-once accessors with bootstrapping re-entrancy guards), and shared utilities (`work_node`, `run_done_payload`, `run_done_list`, `run_name_list`). Also contains the inlined module bodies for `validate`, `subscriptions`, `toposort`, and `seq_chains` — all already-cut-over passes whose Rust side is purely an orchestration shim over Evident FSMs.
**Criticality:** critical (load-time — every self-hosted pass uses this infrastructure; the bootstrapping guard makes it load-path safe)
**Verdict:** not-a-CSP
**Confidence:** high
**How (if replaceable):** This module is orchestration and runtime infrastructure, not a constraint-satisfaction problem. It manages thread-local engine caches, drives Evident FSMs by calling `run_nested`, and bridges `Value` in/out of the pass FSMs. There is no decision problem or property to satisfy here — it is the execution harness that makes the self-hosted passes possible. Replacing it with a Z3 solve would be circular: you need this infrastructure to run the Evident engine that produces Z3 queries.
**Change made:** none
