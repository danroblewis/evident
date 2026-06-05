# `.evidentc` — AOT functionizer cache format (wave 5d)

Companion to `docs/plans/wave-5d-aot-binary-cache.md` — that doc
is the feasibility study; this one is the concrete on-disk
specification the next session implements. Stub.

## File extension and ignore

`.evidentc` files live next to `.smt2` and are git-ignored by
default (`.gitignore`: `.evident/cache/` + `*.evidentc`). Committed
`.evidentc` is opt-in per project (the way `compiler.smt2` is
committed today as a build artifact, but extended to native code
makes the artifact much larger and platform-specific).

## Layout (target)

```
magic     [16 bytes] "evidentc" + 8 reserved bytes
header    [bincode]
  codegen_version   : String
  z3_version        : String
  manifest          : Manifest  (exact same struct kernel/src/manifest.rs)
  step_count        : u32
  smtlib_body_len   : u32
steps     [bincode * step_count]
  Step { var, result_is_bool, is_effects, body_kind, inputs }
  body_kind:
    JitOffset(u64)        ; offset into native section, native blob
    InterpAst(u32)         ; index into the body's parsed AST list
    Residual               ; falls through to z3 each tick
native    [u8 * sum(jit blob sizes)]
  raw object code, one blob per JIT step
smtlib    [u8 * smtlib_body_len]
  the original SMT-LIB body, verbatim
```

`smtlib_body_len` is reserved for the residual+interp rebuild on
load: a fresh Z3 context re-extracts ONLY the non-JIT steps and
re-attaches the cached blobs. This matches the recommendation in
wave-5d §3 option 2 (side-car).

## Lookup

Cache key = `SHA256(canonical post-simplify body) ⊕ SHA256(manifest)`.
Codegen version goes in the FILENAME (`<sha256-prefix>.<v>.evidentc`)
so a codegen bump simply misses every old entry; stale files are
reaped by mtime, never read.

## Status

Not yet implemented. Today the kernel JIT-functionizes every
program in process (`kernel/src/functionize/`); nothing reads or
writes `.evidentc`. The wave-5d landing in this commit is just
the file extension reservation + .gitignore hookup.
