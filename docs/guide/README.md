# Evident programming guides

Practical how-to docs for writing Evident programs against the
runtime as it is today (multi-FSM scheduler, FTI-first I/O).

> **Worked examples** for this repo's primitives live in
> [`examples/`](../../examples/) — every primitive
> has a `test_NN_<name>.ev` you can read or copy from. The
> companion file
> [`examples/COUNTEREXAMPLES.md`](../../examples/COUNTEREXAMPLES.md)
> documents the runtime gaps each demo had to work around.
>
> The demos in that directory follow two repo-specific
> conventions (see CLAUDE.md):
>   1. **Demo files are integration tests.** Each is named
>      `test_*.ev` and bundles the FSM(s) plus inline `sat_*`
>      / `unsat_*` static-test claims. Single-FSM demos are
>      written as multi-FSM programs with one FSM.
>   2. **Demo files don't contain raw FFI calls.** Wrap C in
>      `stdlib/`, then call the wrapper claim from the demo.
>
> These are quality bars for the canonical test set — not
> properties of the language. Your own application code can
> be shaped however suits it.

| Guide | When to read it |
|---|---|
| [`effect-state-machines.md`](effect-state-machines.md) | Before writing any program that uses `evident effect-run`. Explains the step loop, halt convention, the issue/await pattern, and the common pitfalls. |
| [`multi-fsm-programs.md`](multi-fsm-programs.md) | Cookbook for programs composing FSMs through shared world — setup-then-render, stdin echo, graceful shutdown, timer-driven counters, multi-plugin coordination. |
| [`foreign-bindings.md`](foreign-bindings.md) | The Foreign Type Interface (FTI) and the raw FFI escape hatch. Read it when declaring a typed foreign resource (`win ∈ SDL_Window`), wiring a reserved world field to a bridge, writing a new Rust-side bridge, or wrapping a C library in `stdlib/`. |

Read them in that order. Each guide builds on the previous.

## See also

- `docs/design/minimal-runtime.md` — the architectural goals
  (~11K Rust target, FTI-first, libraries-not-plugins).
- `docs/design/foreign-type-interface.md` — the FTI thesis,
  shipped state, and the v3 operation-sequence-validation gap.
- `docs/design/ffi-design.md` — the underlying `Effect::FFICall`
  primitive (escape hatch when no FTI bridge exists yet).
- Library code under `stdlib/{io,sdl,audio,shell}.ev`.
