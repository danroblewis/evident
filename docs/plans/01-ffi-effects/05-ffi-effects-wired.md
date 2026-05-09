# Phase 1.5: Wire FFI effects into the dispatcher

## Goal

Replace the FFI* stub arms in `dispatch_one` with real calls to
`ffi::ffi_open` / `ffi_lookup` / `ffi_call` / `HandleRegistry::close`.

After this lands, an Evident program can issue
`FFICall(handle, "i()", [])` and get back an `IntResult` with a real
value from a C library.

## Prereqs

- Phase 1.3 (dispatcher with stubs) — done.
- Phase 1.4 (step-loop integration) — done, so end-to-end demo works.

## What to build

In `effect_dispatch.rs`, replace the four FFI arms:

```rust
Effect::FFIOpen(path) => {
    match ffi::ffi_open(&ctx.registry, path) {
        Ok(h)  => EffectResult::Handle(h),
        Err(e) => EffectResult::Error(e.0),
    }
}
Effect::FFILookup(lib, sym) => {
    match ffi::ffi_lookup(&ctx.registry, *lib, sym) {
        Ok(h)  => EffectResult::Handle(h),
        Err(e) => EffectResult::Error(e.0),
    }
}
Effect::FFICall(fn_id, sig, args) => {
    let ffi_args: Vec<ffi::FfiArg> = args.iter().map(|a| match a {
        crate::ast::FfiArg::Int(n)    => ffi::FfiArg::Int(*n),
        crate::ast::FfiArg::Bool(b)   => ffi::FfiArg::Bool(*b),
        crate::ast::FfiArg::Str(s)    => ffi::FfiArg::Str(s.clone()),
        crate::ast::FfiArg::Real(r)   => ffi::FfiArg::Real(*r),
        crate::ast::FfiArg::Handle(h) => ffi::FfiArg::Handle(*h),
    }).collect();
    match ffi::ffi_call(&ctx.registry, *fn_id, sig, &ffi_args) {
        Ok(ffi::FfiReturn::Void)        => EffectResult::NoResult,
        Ok(ffi::FfiReturn::Int(n))      => EffectResult::Int(n),
        Ok(ffi::FfiReturn::Bool(b))     => EffectResult::Bool(b),
        Ok(ffi::FfiReturn::Str(s))      => EffectResult::Str(s),
        Ok(ffi::FfiReturn::Real(d))     => EffectResult::Real(d),
        Ok(ffi::FfiReturn::Handle(h))   => EffectResult::Handle(h),
        Err(e) => EffectResult::Error(e.0),
    }
}
Effect::CloseHandle(h) => {
    if ctx.registry.close(*h) {
        EffectResult::NoResult
    } else {
        EffectResult::Error(format!("close: unknown handle {h}"))
    }
}
```

(There are two FfiArg types — one in `ast.rs` for the decoded AST,
one in `ffi.rs` for the FFI primitive. The dispatcher converts.
Could merge later; not a priority.)

## Files touched

- `runtime-rust/src/effect_dispatch.rs`

## Test it

Add to `runtime-rust/tests/effects.rs`:

- Construct an `Effect::FFIOpen("libSystem.dylib")`, dispatch it,
  verify the result is a non-zero `EffectResult::Handle`.
- Take the handle, dispatch `FFILookup(handle, "getpid")`, verify a
  symbol handle.
- Dispatch `FFICall(sym, "i()", [])`, verify the result Int matches
  `std::process::id()`.
- Close the library handle. Verify subsequent FFICall through a
  cached symbol fails (or works, depending — symbols outliving libs
  is documented as caller's problem; verify the behavior is at least
  not a segfault).

## Acceptance

- [ ] Three-step Open → Lookup → Call works end-to-end.
- [ ] All existing tests still pass.
- [ ] LOC: +~50 Rust.

## Notes

The two-FfiArg-types thing is ugly. Worth a refactor pass later. Not
in this task — keep this task focused on wiring.

Error reporting could be richer — currently just a string. For now
match the `EffectResult::Error(String)` shape; can add structured
errors in a future refinement.
