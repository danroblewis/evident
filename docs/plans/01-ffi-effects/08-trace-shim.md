# Phase 1.8: Trace-test record/replay shim for FFI

## Goal

Make trace tests reproducible without needing the actual external
libraries (libSDL, libcurl, etc.). When the trace runner runs a
program, FFI calls get matched against a recorded log instead of
hitting the real library.

This unlocks safe trace testing of all the migrations coming in
Phase 2.

## Prereqs

- Phase 1.5 (FFI wired) — done.
- Phase 1.7 (stdlib/posix.ev exists) — done, gives us a non-trivial
  test target.

## What to build

### Recording mode

Add to `runtime-rust/src/trace_runner.rs`:

```rust
pub struct FFIRecording {
    /// Append-only log of (symbol_path, ArgList → Result) pairs in
    /// the order the program made them. The "symbol_path" is the
    /// last library + symbol name pair, e.g. "libSystem.dylib:getpid"
    /// — captured at FFILookup time and threaded through to the
    /// Call.
    pub calls: Vec<RecordedCall>,
}

pub struct RecordedCall {
    pub library: String,
    pub symbol:  String,
    pub sig:     String,
    pub args:    Vec<crate::ast::FfiArg>,
    pub result:  crate::ast::EffectResult,
}
```

When the trace runner runs in **record** mode (a flag on
`run_trace`), the dispatcher logs every FFI call. After the run,
the recording is serialized to JSON next to the .ev file
(e.g. `programs/demos/ffi_getpid.ev.recording.json`).

### Replay mode

Default mode. The dispatcher consults the recording; for each FFI
call it expects the next entry to match. Mismatch (different
symbol, different args) → trace test fails with a clear message.

Symbol-resolution mode (`FFIOpen` / `FFILookup`) returns sentinel
handles that the replay shim recognizes; never hits the real
`dlopen`. This means trace tests don't need the real library
installed.

### Test runner integration

`evident test` / `evident trace` gains a `--record` flag. CI uses
default replay; updating recordings is an explicit user action.

## Files touched

- `runtime-rust/src/trace_runner.rs` — recording struct + serializer
- `runtime-rust/src/effect_dispatch.rs` — optional shim mode
- `runtime-rust/src/commands/test.rs` — `--record` flag
- A small test program with an FFI call + checked-in recording

## Test it

- Record-then-replay round-trip: run a program in record mode,
  verify the recording file lands. Run again in replay mode, verify
  it matches.
- Tampered recording: edit one arg, verify replay fails with the
  expected mismatch message.
- Missing recording: trace test asks for replay but no .recording
  file exists → fail with clear instruction to run with `--record`.

## Acceptance

- [ ] Record mode produces a deterministic JSON recording.
- [ ] Replay mode runs without touching real libraries.
- [ ] Mismatch detection works.
- [ ] All existing tests still pass.
- [ ] LOC: +~200 Rust (most in trace_runner; the shim itself is small).

## Notes

The recording file format should be human-readable so users can
inspect / hand-edit when debugging. JSON is fine; consider adding a
hash of the program file at the top so an out-of-date recording can
be detected.

There's a question of whether NON-deterministic effects (Time,
ReadLine) should also be recorded. Yes — same machinery applies.
Built-in effects are simpler than FFI to record/replay.

In recording mode, the dispatcher must succeed at the real call to
have a real result to record. So tests in record mode DO need the
external library installed; replay mode doesn't. This is the
intended contract.
