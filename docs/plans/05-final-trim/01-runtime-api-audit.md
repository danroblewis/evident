# Phase 5.1: Runtime API audit

## Goal

`runtime.rs` is 1,189 lines. After Phases 1-4, much of its surface
is dead or redundant (e.g. plugin-related methods). Audit + trim.

## What to do

1. Run `cargo +nightly udeps` and `cargo machete` to find unused
   exports and deps.
2. Manual review: every `pub fn` in `runtime.rs` — is it called by
   library or test code? Drop if not.
3. Look for near-duplicate methods (e.g. `query` vs `query_with_core`
   vs `query_with_program_value` vs ...) — consider whether they
   can collapse.
4. The infrastructure for `query_with_program_and_*` was added for
   self-hosting; after Phase 4.5 simplifies passes, some may go.

## Acceptance

- [ ] LOC: -~400 Rust
- [ ] All tests still pass
