# Phase 5.2: CLI shell trim

## Goal

Slim the CLI command files. After Phase 4.4 moves test reporters to
Evident, `commands/test.rs` (920 lines today) should drop to ~300.

## What to do

1. `commands/test.rs` — remove formatter functions (TAP/JUnit/JSON
   are now Evident libraries that we just call).
2. `commands/common.rs` — consolidate flag parsing; drop helpers no
   longer used.
3. `commands/execute.rs` — remove plugin-driven path (gone in
   Phase 2.5).
4. Audit any remaining commands for dead code.

## Acceptance

- [ ] LOC: -~400 Rust
- [ ] All commands still work end-to-end
