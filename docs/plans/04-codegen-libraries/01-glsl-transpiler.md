# Phase 4.1: GLSL transpiler → stdlib/glsl/

## Goal

Replace `runtime/src/glsl.rs` (1,007 lines, Evident shader AST →
GLSL string) with `stdlib/glsl/transpile.ev` — pure Evident.

## Prereqs

- Phase 3 done (recursive claims + unbounded output + enum bindings).

## What to build

The transpiler walks a shader AST recursively, emitting GLSL source
strings. The Evident version uses recursive claims to walk the AST
and accumulates a `Seq(String)` output.

Sketch:

```evident
import "stdlib/ast.ev"

claim emit_expr(e ∈ Expr, out ∈ String)
    out = match e
        EInt(n)             ⇒ int_to_string(n)
        EBool(b)            ⇒ (b ? "true" : "false")
        EIdentifier(name)   ⇒ name
        EBinary(op, lhs, rhs) ⇒
            ∃ ls, rs ∈ String :
                emit_expr(lhs, ls)
                emit_expr(rhs, rs)
                "(" ++ ls ++ " " ++ binop_glsl(op) ++ " " ++ rs ++ ")"
        ...
```

`int_to_string` and friends become FFI calls to `snprintf` (via
`stdlib/posix.ev`).

## Files touched

- `runtime/src/glsl.rs` — DELETE
- `runtime/src/lib.rs` — drop `pub mod glsl;`
- Wherever the runtime calls into glsl::transpile — replace with
  loading + querying the Evident transpiler claim
- `stdlib/glsl/*.ev` (new)

## Acceptance

- [ ] mario_shader still produces correct GLSL
- [ ] LOC: -1,007 Rust, +~500 Evident

## Notes

This is the largest single LOC win. Also the most complex Evident
program written to date — exercises all of Phase 3's features. Allow
generous time.

Performance: the Evident transpiler will be slower than the Rust
one (every node = one Z3 query). For shaders, that's fine — they
transpile once at load time. Profile to confirm acceptable startup
time.
