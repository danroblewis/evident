# Phase 1.3: Effect dispatcher in executor

## Goal

Add a function in the executor that, given a `Vec<Effect>`, runs each
effect and returns a `Vec<EffectResult>`. Built-ins only at this
stage — FFI* effects route to a stub that returns `Error("FFI not
wired")`. Phase 1.5 wires FFI.

The dispatcher does NOT yet integrate into the per-step solve loop.
That's a separate task. This phase lands the function and tests it
in isolation.

## Prereqs

- Phase 1.2 (Effect/Result types) — done.

## What to build

Add `runtime-rust/src/effect_dispatch.rs`:

```rust
use crate::ast::{Effect, EffectResult, FfiArg};
use crate::ffi::HandleRegistry;

/// Per-runtime-instance state the dispatcher mutates: stdin reader,
/// FFI handle registry, etc. Passed to each dispatch call so the
/// dispatcher itself stays stateless.
pub struct DispatchContext {
    pub registry: HandleRegistry,
    pub stdin:    Box<dyn std::io::BufRead>,
    pub stdout:   Box<dyn std::io::Write>,
    pub start_ms: std::time::Instant,
}

impl DispatchContext {
    pub fn new() -> Self { ... }
}

/// Perform one effect. Returns the matching result.
pub fn dispatch_one(ctx: &mut DispatchContext, e: &Effect) -> EffectResult {
    match e {
        Effect::None         => EffectResult::NoResult,
        Effect::Print(s)     => { write!(ctx.stdout, "{s}").ok(); EffectResult::NoResult }
        Effect::Println(s)   => { writeln!(ctx.stdout, "{s}").ok(); EffectResult::NoResult }
        Effect::ReadLine     => {
            let mut line = String::new();
            match ctx.stdin.read_line(&mut line) {
                Ok(_)  => EffectResult::Str(line.trim_end_matches('\n').to_string()),
                Err(e) => EffectResult::Error(format!("readline: {e}")),
            }
        }
        Effect::Time         => {
            let ms = ctx.start_ms.elapsed().as_millis() as i64;
            EffectResult::Int(ms)
        }
        Effect::Exit(n)      => std::process::exit(*n as i32),
        Effect::FFIOpen(_)
        | Effect::FFILookup(..)
        | Effect::FFICall(..)
        | Effect::CloseHandle(_) => {
            EffectResult::Error("FFI dispatch not yet wired (Phase 1.5)".into())
        }
    }
}

/// Convenience: walk a list, collect results.
pub fn dispatch_all(ctx: &mut DispatchContext, effects: &[Effect]) -> Vec<EffectResult> {
    effects.iter().map(|e| dispatch_one(ctx, e)).collect()
}
```

## Files touched

- `runtime-rust/src/effect_dispatch.rs` (new)
- `runtime-rust/src/lib.rs` — export module

## Test it

Unit tests in the same file:

- `Print` writes to a captured buffer; result is NoResult.
- `Println` adds the newline.
- `ReadLine` reads from a `Cursor` of fake input; result is the line.
- `Time` returns a non-negative Int; calling twice gives non-decreasing values.
- `FFIOpen` returns an Error result with the "not yet wired" message.
- `dispatch_all` preserves order of results.

## Acceptance

- [ ] All listed unit tests pass.
- [ ] All 420+ existing Rust tests still pass (this is purely additive).
- [ ] LOC: +~150 Rust.

## Notes

`DispatchContext` boxes stdin/stdout so unit tests can swap in
in-memory streams. Production wiring will use `io::stdin().lock()`
and `io::stdout().lock()`.

The Exit effect's `process::exit` is fatal — fine for production but
breaks unit tests. Either skip testing Exit, or factor the side effect
through a trait (overkill for v1). Skip is fine.
