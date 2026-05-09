# Phase 4: Codegen + IO library migrations

After Phase 3 lands (recursive claims, unbounded output, enum
pattern bindings), the larger Rust modules can be ported to Evident
libraries.

## Parallel execution

All five tasks are independent. Each touches a different Rust file
and creates a different library. Worktree branches:

```
phase-4-glsl       4.1 GLSL transpiler
phase-4-smtlib-export  4.2 SMT-LIB export
phase-4-smtlib-import  4.3 SMT-LIB import
phase-4-reporters  4.4 Test reporters (TAP/JUnit/JSON)
phase-4-passes     4.5 Inference/desugar consolidation
```

## Per-task plans

- `01-glsl-transpiler.md` — `glsl.rs` (1,007 lines) → `stdlib/glsl/`
- `02-smtlib-export.md` — `smtlib.rs` export half (~500) → `stdlib/smtlib/export/`
- `03-smtlib-import.md` — `smtlib.rs` import half (~450) → `stdlib/smtlib/import/`
- `04-test-reporters.md` — TAP/JUnit/JSON formatters in `commands/test.rs` (~400) → `stdlib/testing/reporters/`
- `05-passes-consolidation.md` — `commands/infer_types.rs` and friends (~700) → smaller Rust glue + bigger `stdlib/passes/`

## Acceptance gate

After all five complete, the runtime should be at ~11K-12K Rust
lines. Phase 5 trims to the 11K target.
