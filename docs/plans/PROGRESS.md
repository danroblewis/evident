# Progress

| Date | LOC | Phase / task | Notes |
|---|---|---|---|
| 2026-05-09 | 17,112 | (baseline) | Roadmap established. |
| 2026-05-09 | 17,623 | Phase 1.1 | FFI primitive landed (commit `3e077ba`). +511. |
| 2026-05-09 | 17,844 | Phase 1.2 | Effect/Result/FfiArg AST types + decoders + tests. +221. stdlib/runtime.ev added (Evident-side enums). |
| 2026-05-09 | 18,112 | Phase 1.3 | effect_dispatch.rs: DispatchContext, dispatch_one (built-ins + FFI wired in same shot — collapsed Phase 1.5 here). 10 unit tests including real libc round-trip. +268. |
| 2026-05-09 | 18,406 | Phase 1.4 | effect_loop.rs: step engine + main shape detection. evaluate_with_extra_assertions multi-pin variant. encode_effect_result_list. +294. |
| 2026-05-09 | 18,631 | Phase 1.6 | effect-run CLI command + effect_hello.ev demo + 3 integration tests. .cargo/config.toml for cross-build env vars. +225. |

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
