# runtime/src/effect_loop/scheduler.rs — Z3-replaceability
**What it does:** The per-tick multi-FSM subscription-driven scheduler: seeds initial state, runs the tick loop (wake-trigger checks, per-FSM Z3 solves, world-snapshot merge, effect dispatch, SpawnFsm handling, async-event blocking), and returns `LoopResult`.
**Criticality:** critical
**Verdict:** hot-path
**Confidence:** high
**How (if replaceable):** This is the innermost hot-path loop — it runs every tick for every frame of every live program. The loop body IS already driving Z3 (calls `rt.query_with_pins_and_given` per FSM per tick); the surrounding scheduler logic (wake-trigger bookkeeping, world-snapshot mutation, pending_changes sets, state encoding, spawn queue) is imperative coordination that cannot be expressed as a single constraint problem. Expressing "which FSMs to wake this tick" as a Z3 solve would require encoding the full subscription state and world-delta history as Z3 terms, adding a solve-per-tick overhead with no benefit over the O(n) boolean flag checks already here.
**Change made:** none
