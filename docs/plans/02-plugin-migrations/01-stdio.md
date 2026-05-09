# Phase 2.1: Stdin/Stdout migration to effects

## Goal

Programs that previously declared `Stdin`/`Stdout`/`CharInput`/
`CharOutput` variables (driven by `StdinPlugin`/`StdoutPlugin`) now
issue `ReadLine`/`Println` effects via the effect dispatcher.

The Rust StdinPlugin and StdoutPlugin code in `executor.rs` (and any
plugins/ files for them) is deleted. ~400 lines out.

## Prereqs

- Phase 1.4 (effect dispatcher in step loop) — done.

## What to build

1. Identify every program in `programs/`, `examples/`,
   `runtime-rust/tests/`, `programs/lang_tests/` that uses Stdin/Stdout.
   Migrate each to the effect-based shape.

2. Write `stdlib/io.ev` — convenience claims wrapping common
   read/print patterns.

3. Delete the StdinPlugin/StdoutPlugin/BatchInput/BatchOutput code
   from executor.rs. Verify the deleted lines compile out cleanly.

4. Update trace_runner so `send "command"` translates to a
   `ReadLine` result in the FFI recording.

## Files touched

- `runtime-rust/src/executor.rs` — delete StdinPlugin and friends
- `runtime-rust/src/trace_runner.rs` — update send-step semantics
- `stdlib/io.ev` (new) — high-level read/print wrappers
- All affected programs (likely 5-10 files)

## Acceptance

- [ ] All previously-stdin-using trace tests pass via the effect path
- [ ] `evident execute` of a stdin-driven text adventure works end-to-end
- [ ] LOC: -~400 Rust, +~50 Evident

## Notes

The text adventure programs (`programs/text_adventure/`) are the
biggest user of Stdin/Stdout. Verify they still play correctly.

Scoping decision: Batch* (StdinLines, StdinAll, etc.) — keep or
drop? The Batch plugin's job is "read all input then run claims
once." That can be expressed as one ReadFile effect plus splitting
in Evident. Drop the Batch plugin in this task; if a use case
breaks, the caller can switch to the new shape.
