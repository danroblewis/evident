# runtime/src/event_sources/file_watcher.rs — Z3-replaceability

**What it does:** Background thread polls a file's mtime at a configurable interval (`EVIDENT_FILE_WATCH_MS`, default 200ms); increments `file_changed: Int` in the world when the mtime changes. Installed when World declares `file_changed: Int` and `EVIDENT_FILE_WATCH` env var is set.

**Criticality:** peripheral

**Verdict:** not-a-CSP

**Confidence:** high

**How (if replaceable):** Pure OS-level time + filesystem stat in a polling loop. A constraint solve has no notion of wall-clock time or `stat()` syscalls. The install decision (two conditions: field name + env var) is too trivial to warrant a Z3 query.

**Change made:** none
