# Phase 1.7: stdlib/posix.ev skeleton

## Goal

Bootstrap the foundational POSIX wrapper library. Other library
migrations (SDL/audio/shader) will import this for shared FFI
plumbing — particularly `libc_handle` (cached libc handle) and
helpers for common calls.

This is Evident-only; no Rust changes. Validates that the FFI is
usable from Evident in a non-trivial way.

## Prereqs

- Phase 1.5 (FFI wired) — done.
- Phase 1.6 (getpid demo) — done, so we know the loop works.

## What to build

`stdlib/posix.ev`:

```evident
import "stdlib/runtime.ev"

-- ── libc handle (lazy single-load) ─────────────────────────────
-- The first effect in any POSIX-using program should resolve
-- libc_handle and stash it in state. Don't call FFIOpen for libc
-- multiple times — it works but wastes handles.

claim libc_path(p ∈ String)
    -- Linux:   "libc.so.6"
    -- macOS:   "libSystem.dylib"
    -- BSD:     "libc.so"
    -- v1: hard-code one. Future: detect via FFICall to uname.
    p = "libSystem.dylib"

-- ── Process basics ─────────────────────────────────────────────

claim getpid_effect(libc ∈ Handle, e ∈ Effect)
    sym ∈ Handle
    -- caller is responsible for resolving sym via FFILookup; this
    -- helper just builds the call effect once you have it.
    e = FFICall(sym, "i()", ⟨⟩)

-- ── File I/O ───────────────────────────────────────────────────

-- O_RDONLY = 0, O_WRONLY = 1, O_RDWR = 2 on POSIX.
claim o_rdonly(n ∈ Int) : n = 0
claim o_wronly(n ∈ Int) : n = 1
claim o_rdwr(n ∈ Int) : n = 2

-- open(path, flags) → fd. Caller looks up the symbol; this builds
-- the effect.
claim open_effect(open_sym ∈ Handle, path ∈ String, flags ∈ Int, e ∈ Effect)
    e = FFICall(open_sym, "i(si)", ⟨ArgStr(path), ArgInt(flags)⟩)

claim close_effect(close_sym ∈ Handle, fd ∈ Int, e ∈ Effect)
    e = FFICall(close_sym, "i(i)", ⟨ArgInt(fd)⟩)

-- read/write skeletons (Phase 2 expands these as plugins migrate)
```

## Files touched

- `stdlib/posix.ev` (new)

## Test it

`tests/lang_tests/test_stdlib_posix.ev`:

- Verify the helper claims load + each value-pin is satisfiable.
- A trace-test program that opens `/etc/hostname` (or a temp file
  written to known content), reads, closes — verifying the file's
  content matches.

```evident
import "stdlib/posix.ev"

claim sat_o_rdonly_is_zero
    n ∈ Int
    o_rdonly
    n = 0
```

## Acceptance

- [ ] `stdlib/posix.ev` parses and conformance tests pass.
- [ ] At least one non-trivial trace test that opens/reads/closes a
      file via FFI through the library works.
- [ ] LOC: +~150 Evident, 0 Rust.

## Notes

The Handle-passing dance (caller looks up sym, helper builds the
effect) is verbose. A cleaner ergonomic design would have the
library cache symbol handles internally — but that needs cross-step
state in libraries, which we don't model yet. Defer.

Platform detection (libc_path) is hard-coded for v1. Acceptable —
`stdlib/posix.ev` will be platform-specific until we have build-time
platform variables. Document the limitation.

The structure `ArgStr / ArgInt / ArgHandle` on every call site is
also verbose. Future ergonomic improvement: an Evident macro or
shorthand for ArgList literals. Out of scope here.
