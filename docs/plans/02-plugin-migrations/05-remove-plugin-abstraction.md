# Phase 2.5: Remove plugin abstraction code

## Goal

After 2.1-2.4 land, the Rust runtime no longer has any built-in
plugins. The `Plugin` trait, the lifecycle dispatch in `executor.rs`,
the auto-detection logic, and `plugins/mod.rs` are dead code.

Strip them.

## Prereqs

- 2.1, 2.2, 2.3, 2.4 all merged.

## What to build

(Nothing — this is pure deletion.)

## Files touched

- `runtime-rust/src/plugins/mod.rs` — delete
- `runtime-rust/src/lib.rs` — drop `pub mod plugins;`
- `runtime-rust/src/executor.rs` — remove plugin lifecycle hooks
  (start/before_step/after_step/stop dispatch loop), default_plugins
  registry, type-name-based auto-detection.

## Acceptance

- [ ] `cargo test` still passes (plugins were optional dispatch; nothing
      depends on them as a type system requirement).
- [ ] LOC: -~400 Rust (lifecycle code in executor + the mod stub).

## Notes

Verify there's no remaining `Plugin` import outside the deleted
files. Some commands/ files might reference plugins; update them to
the effect-based path.

If anything in the deletion fails the build, the migrations weren't
complete. Stop and back-fill the missing migration before continuing.
