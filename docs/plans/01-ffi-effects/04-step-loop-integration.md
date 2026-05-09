# Phase 1.4: Wire effect dispatcher into the step loop

## Goal

Make the executor's per-step solve produce an effect list, dispatch
it, and feed the results into the next step's solve as
`last_results`.

This is the structural change that turns the runtime into an
effect-driven FSM. Existing plugin-driven programs keep working
(plugins are still active); new effect-driven programs become
possible.

## Prereqs

- Phase 1.3 (dispatcher) — done.

## What to build

Modify `runtime-rust/src/executor.rs`:

1. After `mark_system_loads_complete`, detect whether the user's
   `main` claim declares `effects ∈ Seq(Effect)` and `last_results ∈
   Seq(Result)`. If yes, the program is effect-driven; the
   executor uses the new path. If no, fall through to the existing
   plugin-driven path (Phase 2 will eventually delete this
   fallback).

2. New step body:
   - Construct the encoded `last_results` Z3 datatype value from
     the previous step's `Vec<EffectResult>` (initially empty list).
   - Solve `main` with `state` pinned (and `last_results` pinned).
   - Read `state_next` and `effects` from the model.
   - Decode `effects` into `Vec<Effect>` via
     `decode_ast::decode_effect_list`.
   - Dispatch via `effect_dispatch::dispatch_all` → `Vec<EffectResult>`.
   - Move `state ← state_next`. Stash results for next step.
   - Halt when state hits the user-defined halt condition (e.g.
     `state.done = true`).

3. Add encoder for `EffectResult` → Z3 datatype value (so we can
   pin `last_results` in the next step's solve). Same shape as the
   existing AST encoder.

## Files touched

- `runtime-rust/src/executor.rs` — new step-loop path
- `runtime-rust/src/translate/encode_ast.rs` — `encode_effect_result` and `encode_effect_result_list`
- `runtime-rust/src/runtime.rs` — possibly a new `step_with_effects` API

## Test it

`runtime-rust/tests/effects.rs`:

- A program that calls `Println("hello")` once then sets `state.done`.
  Verify stdout has "hello\n" and the executor halts after one step.
- A program that does `ReadLine` then `Println(input)`. Pipe a fake
  string through the dispatcher's stdin; verify echo.
- A program that loops `Time` and stops when elapsed > N. Verify
  result-feedback works.

Plus add a `programs/demos/effect_hello.ev` that runs end-to-end:

```evident
import "stdlib/runtime.ev"

type State(step ∈ Int, done ∈ Bool)

claim main(state, state_next ∈ State,
           last_results ∈ Seq(Result),
           effects ∈ Seq(Effect))
    state.step = 0 ⇒ (state_next.step = 1 ∧ state_next.done = true ∧
                       effects = ⟨Println("hello world")⟩)
    state.step ≠ 0 ⇒ (state_next = state ∧ effects = ⟨⟩)
```

`evident execute programs/demos/effect_hello.ev` should print "hello
world" and exit cleanly.

## Acceptance

- [ ] Effect-loop step path works end-to-end.
- [ ] Existing plugin-driven programs (mario_shader, bouncing_dots,
      text adventures) still execute correctly via the fallback path.
- [ ] All 420+ Rust tests still pass.
- [ ] All 202+ conformance tests still pass.
- [ ] LOC: +~200 Rust (executor changes + encoder).

## Notes

The detection rule "does main declare `effects ∈ Seq(Effect)`?" is a
simple body-walk over the main schema. Document it clearly so users
know how to opt in.

This task DELIBERATELY keeps the plugin path. Phase 2.5 deletes it
once all migrations are done. Mixing is acceptable for the
intermediate state.

Encoding `last_results` for the next solve is the trickiest part —
each step builds a fresh Z3 datatype value and pins it. Reuses the
existing encoder infrastructure.
