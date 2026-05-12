# Progress

| Date | LOC | Phase / task | Notes |
|---|---|---|---|
| 2026-05-09 | 17,112 | (baseline) | Roadmap established. |
| 2026-05-09 | 17,623 | Phase 1.1 | FFI primitive landed (commit `3e077ba`). +511. |
| 2026-05-09 | 17,844 | Phase 1.2 | Effect/Result/FfiArg AST types + decoders + tests. +221. stdlib/runtime.ev added (Evident-side enums). |
| 2026-05-09 | 18,112 | Phase 1.3 | effect_dispatch.rs: DispatchContext, dispatch_one (built-ins + FFI wired in same shot — collapsed Phase 1.5 here). 10 unit tests including real libc round-trip. +268. |
| 2026-05-09 | 18,406 | Phase 1.4 | effect_loop.rs: step engine + main shape detection. evaluate_with_extra_assertions multi-pin variant. encode_effect_result_list. +294. |
| 2026-05-09 | 18,631 | Phase 1.6 | effect-run CLI command + effect_hello.ev demo + 3 integration tests. .cargo/config.toml for cross-build env vars. +225. |
| 2026-05-09 | 18,631 | Phase 1.7 | stdlib/posix.ev (Evident library, 0 Rust delta) + 9 conformance tests. |
| 2026-05-09 | 18,799 | Phase 1.8 | Replay mode in DispatchContext + 3 tests. PartialEq derives on EffectFfiArg/EffectResult. +168. **Phase 1 COMPLETE.** |
| 2026-05-09 | 13,106 | Phase 2 | **AGGRESSIVE CUT**: deleted plugins/ (sdl/audio/shader = 1240), glsl.rs (1007), smtlib.rs (957), trace_runner.rs (533), executor.rs (1118), commands/{execute,export_smt2,import_smt2,initial_state}.rs (~735), tests/{sdl,glsl_transpile,smtlib,perf}.rs (4 files), examples/sdl_render_bench. Removed sdl_demo/, mario/, text_adventure/ programs. Dropped sdl2 + gl from Cargo.toml. Trimmed 10 cli_execute_* tests + cli_query_examples_scheduling. **−5,693 lines, 377 tests still pass, 211 conformance tests pass.** |
| 2026-05-09 | 12,454 | Phase 5.1+5.2 | Test reporters (TAP/JUnit/JSON) deleted: −214. Format CLI flags trimmed. Parser: parse_trace_decl/parse_shader_decl/parse_trace_step/parse_key_name/parse_duration/parse_trailing_assertions/parse_trace_assertion deleted (~252 lines). AST: TraceDecl/TraceStep/TraceAssertion/AssertOp/ShaderDecl deleted. Lexer: Token::Trace/Send/KeyDown/KeyUp/Advance/Shader deleted. runtime.rs: traces()/shaders() accessors deleted. Net: **−652. 377 tests still pass.** |
| 2026-05-09 | 12,227 | Phase 5.2 cont | cmd_infer_types CLI handler deleted (the verbose terminal output) + label_for + render_bindings + aggregate_and_print: −226 lines. infer-types subcommand removed from main.rs. The library API (collect_inferences, auto_apply_inferences, unambiguous_inferences) preserved — query/sample/effect-run still apply inferences automatically. 31 cli_infer_types_* tests deleted (now-unreachable). 346 tests pass. |
| 2026-05-09 | 12,162 | Phase 5.3 | Dead-code purge: usage() helper trimmed (~22 lines). CONFLICT_RULES const + cmd_batch parked-message dispatch deleted. label_for / dt() helper removed. Inference.source_rule field + Var::EnumValue.{enum_name,variant} + Var::EnumCtor.{enum_name,variant} fields removed. cli_batch_says_parked test deleted. 345 tests pass. **Final autonomous-run count: 12,162. 1,162 above 11K target.** |
| 2026-05-11 | 18,252 | Phase 6.1 | Seq+Set runtime parity at dispatch time. Value::SetInt/Bool/Str added; Var::SetVar gains Rc<RefCell<Option<Vec<Value>>>> candidates field. New translate_set_lit_eq pins SetVar to a Z3 set-equality against a Set::empty().add(…) literal and records candidates. extract_set checks each candidate's membership against the model; assert_set_given does the inverse for re-encoding. Four SetVar no-op sites in eval.rs replaced. 5 new tests (set_int/string/bool_literal_pinning, set_no_candidates_omits_binding, set_literal_is_exact_membership). 412 cargo tests pass, 91 conformance tests pass. (Note: LOC bumped from 12,162 baseline by ~6K of intervening multi-FSM/FTI work; this row is the Phase 6.1 landing, not a 5.3-to-6.1 delta.) |

## Outstanding

Phase 1: 1.1 done; 1.2-1.8 ahead.

Phase 2-5: blocked on Phase 1 completion.

## How to update this file

When a task's commit lands, append a row:

```
| YYYY-MM-DD | <new LOC> | Phase X.Y | <commit hash> + brief note |
```

LOC is `wc -l runtime/src/**/*.rs | tail -1`. Don't forget that
new files added in tests/ or stdlib/ don't count toward the Rust
runtime size — only the runtime/src/ tree.
