# Phase 2.2: SDL plugin → stdlib/sdl/ Evident library

## Goal

Replace the 556-line `runtime/src/plugins/sdl.rs` with a pure
Evident library (`stdlib/sdl/`) that calls libSDL2 via FFI.

## Prereqs

- Phase 1 done (FFI primitive + dispatcher).

## What to build

`stdlib/sdl/window.ev`, `stdlib/sdl/event.ev`,
`stdlib/sdl/render.ev` — wrappers around the SDL functions the
existing plugin uses. Read sdl.rs to enumerate them.

Effect-based shape: Evident issues `FFICall(SDL_PollEvent, ...)`,
gets back a Handle to an event struct, then calls per-field
accessors (also via FFI) to read the event type/data.

Migrate `programs/sdl_demo/*.ev` to the new library. Verify
bouncing_dots still bounces, anchor_collect still plays.

Delete `plugins/sdl.rs` and `plugins/audio.rs` if not separately
needed (audio is Phase 2.3).

Cargo.toml: drop `sdl2` and `gl` Rust deps. SDL2 still needs to be
installed at the system level (libSDL2.dylib / libSDL2.so) for the
runtime to dlopen it.

## Files touched

- `runtime/src/plugins/sdl.rs` — delete
- `runtime/Cargo.toml` — drop sdl2, gl
- `stdlib/sdl/*.ev` (new, multiple files)
- `programs/sdl_demo/*.ev` — migrated

## Acceptance

- [ ] bouncing_dots and anchor_collect run from Evident library
- [ ] LOC: -556 Rust, +~400 Evident

## Notes

This is the biggest single-task LOC win. The SDL plugin is also
the most complex one — events, render loop, input mapping.

If the FFI call cost per frame is too high (libffi has per-call
overhead), consider exposing a "render frame" composite effect that
batches multiple SDL calls. Profile first.

Shader-related SDL setup is in Phase 2.4.
