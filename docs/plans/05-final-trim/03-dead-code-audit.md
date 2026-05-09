# Phase 5.3: Dead-code audit

## Goal

Final pass to hit the 11K LOC target. Catch anything earlier phases
missed.

## What to do

1. `cargo clippy --all-targets -- -D warnings` and address every
   `dead_code` warning by either using or deleting.
2. Manual `grep` for AST variants that no longer have any
   constructor in the current source/test set.
3. Look for "compatibility shims" — methods kept for backward
   compatibility that could now be removed.
4. Audit `Cargo.toml` for unused dependencies (`cargo machete`).

## Acceptance

- [ ] `wc -l runtime-rust/src/**/*.rs` shows ≤ 11,000
- [ ] `cargo build --release` is warning-free
- [ ] `cargo test` passes

If after this the count is still > 11,000, document in
`docs/plans/PROGRESS.md` why and what remaining cuts are possible.
