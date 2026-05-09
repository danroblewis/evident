# Phase 1.2: Effect + Result AST types

## Goal

Define the `Effect` and `Result` types that connect Evident programs
to the runtime's effect dispatcher. After this lands, programs can
declare effect/result variables; the dispatcher (Phase 1.3) and
built-in handlers (Phase 1.4) wire actual behavior.

This task is types-only. No runtime behavior changes; existing tests
keep passing.

## Prereqs

- Phase 1.1 (FFI primitive) — done.

## What to build

Two parallel artifacts:

### A. `stdlib/runtime.ev`

New file. Declares the Effect and Result enums plus the EffectList /
ResultList tail-recursive list types (mirroring stdlib/ast.ev's
LinkedList style).

```evident
-- Effect: a single side-effect the runtime should perform between
-- solver steps. Built-in effects (Print/ReadLine/Time/Exit) hit the
-- OS directly. FFI* effects route through libffi; see
-- docs/design/ffi-design.md.
enum Effect =
    None
    Print(String)
    Println(String)
    ReadLine
    Time
    Exit(Int)
    FFIOpen(String)
    FFILookup(Handle, String)
    FFICall(Handle, String, ArgList)
    CloseHandle(Handle)

-- Result: the outcome of one performed effect. Position-aligned
-- with the previous step's effect list.
enum Result =
    NoResult
    IntResult(Int)
    StringResult(String)
    BoolResult(Bool)
    RealResult(Real)
    HandleResult(Handle)
    Error(String)

-- Lists.
enum EffectList = ELNilEff ; ELConsEff(Effect, EffectList)
enum ResultList = RLNil ; RLCons(Result, ResultList)

-- One FFI call argument, tagged with its Evident type.
enum FFIArg =
    ArgInt(Int)
    ArgBool(Bool)
    ArgStr(String)
    ArgReal(Real)
    ArgHandle(Handle)
enum ArgList = ALNil ; ALCons(FFIArg, ArgList)

-- Opaque library / symbol / pointer. The runtime tracks these in
-- its HandleRegistry; Evident programs can only pass them around
-- and Close them.
type Handle = Int   -- u64 IDs; 0 is the null sentinel
```

### B. Mirror types in Rust AST

The Rust executor needs to read decoded Effect/Result values to
dispatch them. So the same shapes need:

- A Rust enum `Effect` mirroring the Evident enum (so the dispatcher
  can `match effect { ... }`).
- A Rust enum `Result` (or `EffectResult` to avoid clashing with
  `std::result::Result`) for the outcomes.
- A small decoder in `runtime-rust/src/translate/decode_ast.rs` that
  reads the Z3 datatype value into the Rust enum.

Add to `runtime-rust/src/ast.rs` (or a new `runtime-rust/src/effect.rs`
if `ast.rs` is getting crowded — `ast.rs` is 350 lines, fine to add
~50 more):

```rust
/// One side-effect, materialized from a Z3 datatype value of type
/// stdlib `Effect`. Drives the executor's effect-dispatch loop.
#[derive(Debug, Clone)]
pub enum Effect {
    None,
    Print(String),
    Println(String),
    ReadLine,
    Time,
    Exit(i64),
    FFIOpen(String),
    FFILookup(u64, String),
    FFICall(u64, String, Vec<FfiArg>),
    CloseHandle(u64),
}

#[derive(Debug, Clone)]
pub enum FfiArg {
    Int(i64),
    Bool(bool),
    Str(String),
    Real(f64),
    Handle(u64),
}

#[derive(Debug, Clone)]
pub enum EffectResult {
    NoResult,
    Int(i64),
    Str(String),
    Bool(bool),
    Real(f64),
    Handle(u64),
    Error(String),
}
```

Decoder additions in `decode_ast.rs`:

```rust
pub fn decode_effect(v: &Value) -> Result<Effect> { ... }
pub fn decode_effect_list(v: &Value) -> Result<Vec<Effect>> { ... }
pub fn decode_ffi_arg(v: &Value) -> Result<FfiArg> { ... }
pub fn decode_ffi_arg_list(v: &Value) -> Result<Vec<FfiArg>> { ... }
```

(No encoder needed yet — results will be encoded into Z3 datatypes
when the dispatcher runs in Phase 1.3.)

## Files touched

- `stdlib/runtime.ev` (new)
- `runtime-rust/src/ast.rs` (or new `effect.rs`)
- `runtime-rust/src/lib.rs` (export new module if separate file)
- `runtime-rust/src/translate/decode_ast.rs`

## Test it

- Add round-trip tests in `runtime-rust/tests/roundtrip_ast.rs` for
  Effect / FfiArg encode + decode (encoder added later — for now,
  hand-construct a Z3 datatype value matching `Println("hi")` and
  verify decode produces the right Rust enum).
- Add a stdlib unit-load test: `evident test stdlib/runtime.ev`
  parses and loads cleanly.

## Acceptance

- [ ] `stdlib/runtime.ev` parses and round-trips through the existing
      Encoder/decoder for the parts that go through them.
- [ ] Rust `Effect` enum present, decoders working.
- [ ] All existing 420 Rust tests still pass.
- [ ] All 202 conformance tests still pass.
- [ ] LOC delta: +~120 Rust (types + decoder), +~50 Evident.

## Notes

The `Handle` type is just `Int` for v1. We're not doing distinct
opaque types yet because Evident's type system doesn't support
nominal subtyping of primitives. A Handle is conventionally an Int;
the runtime validates handle IDs against the registry at FFI call
time.

Naming: `ELNilEff` instead of `ELNil` because `ELNil`/`ELCons` are
already taken by stdlib/ast.ev's `ExprList`. The variant namespace is
global. Same for `RLNil` — pick a unique prefix.

Actually verify the namespace conflict before committing — grep
stdlib/ast.ev for any conflicting variant names.
