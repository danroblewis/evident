# Phase 1.1: FFI primitive ✅ DONE (commit `3e077ba`)

The runtime's `dlopen` + `dlsym` + libffi-call wrapper. Lives in
`runtime/src/ffi.rs`.

Validates end-to-end with libc round-trips: `getpid`, `strlen`,
`abs`. Signature parser catches type mismatches before any unsafe
call. HandleRegistry manages library/symbol/pointer lifetimes via
optional drop closures.

9 unit tests pass. +511 lines Rust (the FFI primitive plus tests
and docs).

This task is complete. Subsequent Phase 1 tasks depend on it.
