# Evident programming guides

Practical how-to docs for writing real Evident programs against the
runtime as it is today (effect-driven, FFI-based).

| Guide | When to read it |
|---|---|
| [`effect-state-machines.md`](effect-state-machines.md) | Before writing any program that uses `evident effect-run`. Explains the step loop, halt convention, the issue/await pattern, and the common pitfalls. |
| [`ffi-bindings.md`](ffi-bindings.md) | When wrapping a C library (or extending an existing wrapper) — `LibCall` shape, signature codes, ArgList encoding, library paths, debugging. |

Read them in that order. The FFI guide assumes you understand the
state-machine model.

## See also

- `docs/design/minimal-runtime.md` — the architectural goals
  (~11K Rust target, FFI-first, libraries-not-plugins).
- `docs/design/ffi-design.md` — the FFI primitive's design rationale.
- Worked examples under `programs/demos/effect_*.ev`.
- Library code under `stdlib/{io,sdl,audio,shell}.ev`.
