# Hybrid coverage of `examples/test_*.ev`

> Generated/maintained by the **hybrid-coverage** session. Tracks which example
> programs run **end-to-end byte-identical** through the hybrid pipeline
> (`runtime_smt::transpile_fsm` → SMT-LIB fixture → `scheduler::run` greenfield
> engine) versus the legacy oracle (`evident effect-run <file> --max-steps N`).
>
> Re-check any time with `runtime-smt/baseline.sh [glob]` — it runs both paths
> and compares stdout + exit byte-for-byte. The convergence integration tests in
> `runtime-smt/tests/convergence.rs` pin the byte-identical ones in-process.

## Verdict legend
- **HYBRID ✓** — runs through the hybrid, stdout + exit byte-identical to the oracle.
- **GAP** — feasible with bounded transpiler/engine work; not yet done.
- **OUT** — genuinely out of scope for this engine (with the precise blocker).

## Status

| Example | Verdict | Notes |
|---|---|---|
| test_01_hello            | HYBRID ✓ | scalar/enum, Println/Exit |
| test_02_counter          | HYBRID ✓ | payload enum `Count(Int)` + `match` payload binding (Phase B) |
| test_03_seq_chain        | HYBRID ✓ | enum state, seq-literal effects |
| test_04_parse_int        | HYBRID ✓ | `ParseInt`, `last_results` match on `IntResult`/`ErrorResult` (Phase A) |
| test_05_int_to_str       | HYBRID ✓ | `IntToStr`, `last_results` → `StringResult` (Phase A) |
| test_06_shell_run        | OUT | `ShellRun "date"` → non-deterministic wall-clock output; no byte-identical |
| test_07_time             | OUT | `Time`/`MonotonicTime` wall clock; engine stubs to a constant → differs from oracle |
| test_08_exit_code        | HYBRID ✓ | enum state, Exit(42) |
| test_09_two_fsms         | GAP | two FSMs over shared `world`, payload enum `PTick(Int)` (Phase C) |
| test_10_spawn            | OUT | process spawn / payload enum + spawn effect; no spawn dispatch in engine |
| test_11_frameclock       | OUT | async FrameClock event source — engine has no async sources |
| test_12_hostname         | OUT | FTI hostname (FFI) → non-deterministic host-dependent output |
| test_13_timer            | OUT | async Timer FTI — no async sources |
| test_14_stdin            | OUT | async stdin source — no async sources / external input |
| test_15_signal           | OUT | async SIGINT source — no async sources |
| test_16_sdl_red          | OUT | SDL FFI + display |
| test_17_sdl_triangle     | OUT | SDL/GL FFI + display |
| test_18_reflection       | OUT | `Program` AST reflection world-plugin — no reflection infra |
| test_19_prev_tick        | HYBRID ✓ | enum+scalar state, `last_results[1]`, `#last_results`, `++` concat (Phase A) |
| test_20_pure_counter     | HYBRID ✓ | scalar-only state, nested ternary effects, `last_results` (Phase A) |
| test_22_prev_record      | GAP | record-typed state (`type` with fields) — needs record transpile |
| test_24_sdl_mixer        | OUT | SDL_mixer FFI |
| test_25_per_component_jit| GAP/OUT? | enum state + records; perf demo (triage: feasible w/ records) |
| test_26_value_cache      | GAP | two auto-scheduled FSMs + shared world (Phase C-ish, multi-FSM) |
| test_27_parallel_solving | GAP | single FSM, inline claims + enum + `≠` constraints |
| test_28_parallel_enum_coloring | GAP | enum state via ternary + scalar `tick` + multi-name enum decls + `≠` (Phase D) |
| test_29_jit_heavy_compute| GAP | enum state via ternary + scalar `tick` + arithmetic chain (Phase D) |
| test_30_jit_gap_closures | GAP | records + String concat + div/mod (harder) |
| test_31_symbolic_regression | OUT | output is the SymbolicFunctionizer's discovered formula — strategy-dependent |
| test_32_llm_functionizer | OUT | LLM functionizer + stdin |
| test_33_satisfier        | OUT | satisfier-mode PRNG-drawn values; Z3 picks different assignment → stdout values differ |
| test_34_halts_within     | OUT | embedded `halts_within(F,N)` execution model — scheduler engine has no embedded-run |
| test_35_run_fsm          | OUT | embedded `run(F,init)` execution model |
| test_36_sum_tree         | OUT | embedded run + recursive payload enums (Tree/Stack) |
| test_37_tree_walk        | OUT | embedded run + recursive payload enums + String labels |
| test_38_nested_effects   | OUT | embedded run with effect percolation to parent |
| test_39_string_ops       | GAP | string ops (`index_of`/`substr`/`replace`) → Z3 string theory lowering |

## Tally (current)
HYBRID ✓: 7 · GAP: ~11 · OUT: ~19  (of 37)

Phases remaining: B (payload enums → test_02), C (multi-FSM/world → test_09, test_26),
D (enum-ternary constraint loops → test_28, test_29), and string ops (test_39).
The `OUT` set is the honest boundary: async sources, FFI/SDL/GL, wall-clock/host
non-determinism, reflection, strategy-dependent solver output, and the embedded
`run(F,init)`/`halts_within` execution model (a different runtime tier than the
SMT-LIB scheduler engine).
