# runtime/src/runtime/sample.rs — Z3-replaceability
**What it does:** Exposes the `sample` API: builds a fresh Z3 solver for the named schema and calls `sample_cached_inner` to enumerate up to `n` distinct satisfying models via successive blocking clauses.
**Criticality:** peripheral (used only by the `evident sample` CLI subcommand, not on any per-tick path)
**Verdict:** trivial
**Confidence:** high
**How (if replaceable):** The file is 30 lines and is a thin wrapper: it resolves the schema, forwards to `build_cache` and `sample_cached_inner` (both defined in `translate/`), and returns the results. There is no algorithm here to replace with a constraint — the actual multi-model enumeration logic lives in `translate/`. This adapter is too small to be worth a solve.
**Change made:** none
