# Evident programming guides

Practical how-to docs for writing Evident programs against the
runtime as it is today (multi-FSM scheduler, FTI-first I/O).

> **Worked examples** for this repo's primitives live in
> [`programs/demos/`](../../programs/demos/) — every primitive
> has a `test_NN_<name>.ev` you can read or copy from. The
> companion file
> [`programs/demos/COUNTEREXAMPLES.md`](../../programs/demos/COUNTEREXAMPLES.md)
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
| [`ffi-bindings.md`](ffi-bindings.md) | For stdlib authors wrapping a C library (or extending an existing wrapper) — `LibCall` shape, signature codes, ArgList encoding, library paths, debugging. **Programs should never need this guide directly.** |

Read them in that order. Each guide builds on the previous.

## See also

- `docs/design/minimal-runtime.md` — the architectural goals
  (~11K Rust target, FFI-first, libraries-not-plugins).
- `docs/design/ffi-design.md` — the FFI primitive's design rationale.
- `docs/design/foreign-type-interface.md` — FTI bridge design,
  the typed-resource alternative to threading FFI handles
  through state.
- Library code under `stdlib/{io,sdl,audio,shell}.ev`.
