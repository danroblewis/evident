# runtime/src/effect_loop/fsm.rs — Z3-replaceability
**What it does:** Resolves `fsm`-keyword schemas into `MainShape` structs by walking body items to identify state-pair, effects/last_results slots, world var, FTI params, and event subscriptions. Also provides `all_fsms` (sorts writers before readers) and `full_world_access` (transitive passthrough walk for access sets).
**Criticality:** critical
**Verdict:** not-a-CSP
**Confidence:** high
**How (if replaceable):** This is a pure AST-slot-resolution walk, not a constraint problem. The "find the state pair" logic is a scan over `BodyItem::Membership` looking for `name`/`name_next` patterns; there is no satisfying-assignment to find. The CLAUDE.md explicitly notes that shape-detection was killed (the `fsm` keyword is the sole signal); re-introducing a solver here would risk resurrecting the rejected detect-by-shape model. `full_world_access` is similarly a graph reachability walk (transitive passthrough expansion), which a solve would handle far more expensively than a DFS.
**Change made:** none
