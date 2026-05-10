# Evident programming guides

Practical how-to docs for writing real Evident programs against the
runtime as it is today (effect-driven, FFI-based).

| Guide | When to read it |
|---|---|
| [`effect-state-machines.md`](effect-state-machines.md) | Before writing any program that uses `evident effect-run`. Explains the step loop, halt convention, the issue/await pattern, and the common pitfalls. |
| [`multi-fsm-programs.md`](multi-fsm-programs.md) | Once you understand a single FSM. Cookbook for programs with multiple FSMs coordinating via shared world — setup-then-render, stdin echo, graceful shutdown, timer-driven counters, multi-plugin coordination. |
| [`ffi-bindings.md`](ffi-bindings.md) | When wrapping a C library (or extending an existing wrapper) — `LibCall` shape, signature codes, ArgList encoding, library paths, debugging. |

Read them in that order. Each guide builds on the previous.

## See also

- `docs/design/minimal-runtime.md` — the architectural goals
  (~11K Rust target, FFI-first, libraries-not-plugins).
- `docs/design/ffi-design.md` — the FFI primitive's design rationale.
- Worked examples under `programs/demos/effect_*.ev`.
- Library code under `stdlib/{io,sdl,audio,shell}.ev`.
