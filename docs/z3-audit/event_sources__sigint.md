# runtime/src/event_sources/sigint.rs — Z3-replaceability

**What it does:** SIGINT (Ctrl-C) bridge. Registers a signal handler via `signal_hook`, spawns a thread that blocks on `signals.forever()`, and sends a scheduler tick (plus optional `signal_received: Int` world write) on each SIGINT received.

**Criticality:** peripheral

**Verdict:** not-a-CSP

**Confidence:** high

**How (if replaceable):** Blocking on OS signal delivery is inherently external IO — no constraint formulation can replace `Signals::forever()`. The `install_world_plugin` decision (world field check + subscription set membership) is a 2-condition boolean; a Z3 query adds only overhead.

**Change made:** none
