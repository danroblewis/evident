# runtime/src/event_sources/wall_clock.rs — Z3-replaceability

**What it does:** Wall-clock bridge. Spawns a thread that writes `SystemTime::now()` as Unix-ms to the `now_ms: Int` world field on each interval tick. First write is immediate (no initial sleep). Installed when World declares `now_ms: Int`; interval from `EVIDENT_CLOCK_MS` (default 100ms).

**Criticality:** peripheral

**Verdict:** not-a-CSP

**Confidence:** high

**How (if replaceable):** Reading the system clock and blocking `thread::sleep` are pure OS calls. A constraint solve has no mechanism to observe wall-clock time. The single install condition (`has_world_field("now_ms", "Int")`) is a trivial HashMap lookup.

**Change made:** none
