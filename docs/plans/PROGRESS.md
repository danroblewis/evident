# Progress

| Date | LOC | Phase / task | Notes |
|---|---|---|---|
| 2026-05-09 | 17,112 | (baseline) | Roadmap established. |
| 2026-05-09 | 17,623 | Phase 1.1 | FFI primitive landed (commit `3e077ba`). +511. |

## Outstanding

Phase 1: 1.1 done; 1.2-1.8 ahead.

Phase 2-5: blocked on Phase 1 completion.

## How to update this file

When a task's commit lands, append a row:

```
| YYYY-MM-DD | <new LOC> | Phase X.Y | <commit hash> + brief note |
```

LOC is `wc -l runtime-rust/src/**/*.rs | tail -1`. Don't forget that
new files added in tests/ or stdlib/ don't count toward the Rust
runtime size — only the runtime-rust/src/ tree.
