# Phase 4.2: SMT-LIB export → stdlib/smtlib/export/

## Goal

Replace the export half of `runtime-rust/src/smtlib.rs` (~500 lines)
with `stdlib/smtlib/export.ev`. Same shape as the GLSL transpiler:
recursive AST walk producing a string.

## Prereqs

- Phase 3 done.
- Phase 4.1 (GLSL) gives a worked template.

## What to build

`stdlib/smtlib/export.ev` — emit `(declare-fun ...)` and `(assert
...)` from a Program AST. SMT-LIB is simpler than GLSL — fewer
expression forms.

Update the `evident export-smt2` CLI to load + call the Evident
transpiler instead of the Rust function.

## Files touched

- `runtime-rust/src/smtlib.rs` — delete the export half
- `runtime-rust/src/commands/export_smt2.rs` — call Evident library
- `stdlib/smtlib/export.ev` (new)

## Acceptance

- [ ] Round-tripping a simple program through export + Z3 still works
- [ ] LOC: -500 Rust, +~250 Evident
