# runtime/src/stdlib_path.rs — Z3-replaceability
**What it does:** PYTHONPATH-style resolver for the Evident `stdlib/` directory; checks env overrides (`EVIDENT_STDLIB`/`EVIDENT_STDLIB_DIR`), XDG paths, install paths, and dev-tree fallbacks by testing for a marker file (`runtime.ev`).
**Criticality:** peripheral
**Verdict:** not-a-CSP
**Confidence:** high
**How (if replaceable):** Pure filesystem + env-var I/O; no constraint structure whatsoever. Z3 cannot query the filesystem.
**Change made:** none
