# Phase 2.4: Shader plugin → stdlib/shader/

## Goal

Replace `runtime/src/plugins/shader.rs` (443 lines) — the GL
3.3 shader compilation/binding/render loop — with `stdlib/shader/`
that calls libGL via FFI.

NOTE: This task migrates the shader RUNTIME (compiling + binding +
drawing). The SHADER TRANSPILER (Evident shader AST → GLSL string)
is `glsl.rs` and lives in Phase 4.1 because it needs Phase 3
prerequisites.

## Prereqs

- Phase 2.2 (SDL library, for window context)

## What to build

- `stdlib/shader/program.ev` — glCreateProgram, glAttachShader,
  glLinkProgram via FFI.
- `stdlib/shader/draw.ev` — glDraw* calls.
- Update mario_shader and other shader demos to use the new
  library. They still depend on the GLSL transpiler being in Rust
  for now — that's Phase 4.

## Files touched

- `runtime/src/plugins/shader.rs` — delete
- `runtime/Cargo.toml` — drop `gl` (already dropped if 2.2 took it)
- `stdlib/shader/*.ev` (new)
- mario_shader migrated

## Acceptance

- [ ] mario_shader still hits 60fps
- [ ] LOC: -443 Rust, +~250 Evident

## Notes

OpenGL has a lot of state-setting calls. Each becomes its own FFI
call, which could be slow. If perf suffers, consider grouping into
composite effects (e.g. `BindAndDraw(program, vao, count)`).

Resource lifetimes (texture handles, VAO IDs) need careful CloseHandle
discipline.
