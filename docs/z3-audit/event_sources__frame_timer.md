# runtime/src/event_sources/frame_timer.rs — Z3-replaceability

**What it does:** Periodic tick source. Spawns a thread that sleeps for a configured interval then sends a `SchedulerEvent::Tick` and optionally writes an incrementing `tick_count: Int` to the world. The `install_world_plugin` function installs it when `EVIDENT_TICK_MS` is set, World has `tick_count: Int`, or an FSM subscribes to the `tick` event.

**Criticality:** critical

**Verdict:** not-a-CSP

**Confidence:** high

**How (if replaceable):** The loop body is `thread::sleep(interval)` — real wall-clock blocking. A Z3 solve cannot represent "wait N milliseconds then fire." The install decision (3-way OR: env var, world field, FSM subscription set) is a fast boolean check with no CSP value.

**Change made:** none
