# Round 14 — Gate accepts external (FTI-bridged) types

**Outcome:** STRUCTURAL CHANGE. The function-izer gate now accepts
`external type X` declarations (SDL_Window, GL_Program, etc.) as
opaque passthroughs. No new cross-example HITs yet — the next
blocker for Mario's keyboard/display FSMs is positional claim-call
inlining (`sdl_pump_events(pump_eff)`).

## What changed

### Gate: external types pass

```rust
// Before: every Membership of a non-primitive type had to
// recursively pass is_simple_record_rec. SDL_Window failed
// because its body has Constraint items (the `install = ⟨...⟩`
// pin) which are categorically refused.

// After: external types are accepted unconditionally as opaque.
if decl.external { return true; }
```

The premise: an `external type` is an FTI bridge — its body is
metadata for the Rust-side bridge installer, not data the
function-izer needs to translate. The bridge writes the leaf
fields (`win.handle`, `win.renderer`, …) into the world snapshot;
those fields flow through `given` like any other primitive.

### Chain extraction: skip the bare name

`try_extract_one_chain` previously treated every Membership-declared
name as a substitution target. For an external-typed name like `win`,
there's no `win = expr` substitution — so the chain failed.

Fix: when collecting target vars, skip Memberships whose declared
type is external. The leaf fields (`win.handle`, etc.) appear in
`given` already; the bare name needs no substitution.

```rust
if is_external_type(type_name) { continue; }
```

### Also: `"Nat"` accepted as primitive

Was previously refused (only `Int|Real|Bool|String` were
recognized). Now `Nat` joins the list — it's just a non-negative
Int at the type-system level, and demos use it as a counter type.

## What this unlocked

Mario rejection trace, before/after:

```
                Before                              After
display:    Membership win∈SDL_Window         body Call sdl_delay
keyboard:   Membership win∈SDL_Window         body Call sdl_pump_events
game:       Forall (non-static bounds)        Forall (non-static bounds)
level_gen:  non-Eq Binary op Implies          non-Eq Binary op Implies
```

The SDL_Window membership is no longer the blocker. New blocker
for keyboard/display: `Expr::Call(claim_name, args)` body
Constraints — positional claim invocations like
`sdl_pump_events(pump_eff)` or `sdl_delay(16, delay_eff)`.

## Test impact

- All 444 cargo tests pass with and without `EVIDENT_FUNCTIONIZE=0/1`.
- All 119 conformance tests pass in both modes.
- Cross-example HIT count unchanged (no demos crossed the new
  threshold yet — they all hit the Call-as-Constraint rejection
  next).

## Round 15 candidates

1. **Positional claim-call inlining** (highest impact). Add an
   AST pre-pass that detects `Constraint(Call(name, args))` for
   known claims and inlines the called claim's body with args
   substituted for params. Same logic as the Z3 translator's
   `inline.rs` does at the Z3 level, just at AST level. Would
   unlock keyboard + display.
2. **Forall-over-Seq** (Mario game). Needs Field/Index resolution
   and seq-length pinning.
3. **Implies as guarded substitution** (Mario level_gen). Tricky
   soundness.

Recommend Round 15 = #1, since it likely unlocks two Mario FSMs
and possibly many stdlib-wrapped demos.
