# Phase 5: Final trim

After Phases 1-4 land, audit and trim the remaining Rust to hit the
11K target.

## Per-task plans

- `01-runtime-api-audit.md` — `runtime.rs` (1,189 lines) audit;
  remove dead methods, dedup near-duplicates.
- `02-cli-shell-trim.md` — CLI command files; remove formatters
  already moved to Evident; consolidate flag parsing.
- `03-dead-code-audit.md` — final pass: clippy + manual grep for
  unreferenced functions, dead enum variants, unused warnings.

## Sequential

Each builds on the previous. Run in order, in one branch.

## Acceptance gate

After Phase 5: `wc -l runtime/src/**/*.rs` ≤ 11,000.
