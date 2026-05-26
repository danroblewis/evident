# runtime/src/event_sources/file_line_reader.rs — Z3-replaceability

**What it does:** Background thread reads lines from a file path (from `EVIDENT_FILE_INPUT` env var), queuing each line as a world write to `file_line: String` plus optional `file_seq: Int` and `file_eof: Bool`. Installed when World declares `file_line: String` and the env var is set.

**Criticality:** peripheral

**Verdict:** not-a-CSP

**Confidence:** high

**How (if replaceable):** Reads bytes from an OS file descriptor in a blocking thread — inherently stateful, time-ordered IO. A constraint solve cannot block on a file read or produce the next line of input. The `install_world_plugin` decision logic (check two world field names + an env var) is a trivial 3-way lookup with no CSP value.

**Change made:** none
