# runtime/src/event_sources/stdin.rs — Z3-replaceability

**What it does:** Stdin line-reader bridge. Spawns a thread that calls `BufRead::read_line` in a loop, queuing each line as `stdin_line: String` (and optional `stdin_seq: Int`) world writes with a scheduler wake. Also detects and rejects concurrent `Effect::ReadLine` use (fd 0 race).

**Criticality:** peripheral

**Verdict:** not-a-CSP

**Confidence:** high

**How (if replaceable):** Blocking line reads on fd 0 are fundamentally OS IO; a constraint solve cannot block until the user types. The conflict-detection logic (check if any FSM uses `ReadLine` identifier) is a simple string lookup with no search problem structure.

**Change made:** none
