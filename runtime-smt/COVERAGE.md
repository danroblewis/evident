# Hybrid coverage of `examples/test_*.ev`

> Maintained by the **hybrid-coverage** session. Tracks which example programs
> run **end-to-end byte-identical** through the hybrid pipeline
> (`runtime_smt::transpile_fsm` → SMT-LIB fixture → `scheduler::run` greenfield
> engine) versus the legacy oracle (`evident effect-run <file> --max-steps N`).
>
> Re-check any time with `runtime-smt/baseline.sh [glob]` — it runs both paths
> and compares stdout + exit byte-for-byte. Each `HYBRID ✓` row is also pinned
> in-process by a `#[test]` in `runtime-smt/tests/convergence.rs`.

## Verdict legend
- **HYBRID ✓** — runs through the hybrid; stdout + exit **byte-identical** to the oracle.
- **GAP** — *feasible* with bounded, additive transpiler/engine work; not yet done. The
  engine has no wrong behavior here — the front-end transpiler declines the shape.
- **OUT** — *genuine boundary*: cannot be byte-identical with reasonable effort, with
  the precise blocker. (Non-determinism, FFI/SDL/GL, async event sources, a different
  execution tier, or strategy-dependent solver output.)

## Status (12 ✓ · 5 GAP · 20 OUT, of 37)

| Example | Verdict | Detail |
|---|---|---|
| test_01_hello            | **HYBRID ✓** | enum state, Println/Exit |
| test_02_counter          | **HYBRID ✓** | payload enum `Count(Int)` + `match` payload binding |
| test_03_seq_chain        | **HYBRID ✓** | enum state, multi-element seq-literal effects |
| test_04_parse_int        | **HYBRID ✓** | `ParseInt` + `match last_results` on `IntResult`/`ErrorResult` |
| test_05_int_to_str       | **HYBRID ✓** | `IntToStr` + `last_results` → `StringResult` |
| test_08_exit_code        | **HYBRID ✓** | enum state, `Exit(42)` |
| test_09_two_fsms         | **HYBRID ✓** | TWO FSMs over shared `world`, payload enum `PTick(Int)`, world read/write |
| test_19_prev_tick        | **HYBRID ✓** | enum+scalar state, `last_results[1]`, `#last_results`, `++` concat |
| test_20_pure_counter     | **HYBRID ✓** | scalar-only state, nested ternary effects, `last_results` |
| test_28_parallel_enum_coloring | **HYBRID ✓** | enum-ternary state + scalar `tick` + 144 multi-name enum vars + 576 `≠` |
| test_29_jit_heavy_compute| **HYBRID ✓** | enum-ternary state + scalar `tick` + ~90-var arithmetic chain |
| test_39_string_ops       | **HYBRID ✓** | `index_of`/`substr`/`replace` → Z3 `str.*`, `++` |
| --- | --- | --- |
| test_22_prev_record      | GAP | record-typed FSM state (`type` with fields as the state) — needs record state transpile |
| test_25_per_component_jit| GAP | `Set(Int)` intermediate sort + record world — needs set/record transpile |
| test_26_value_cache      | GAP | `is_first_tick` used in a *derived* Bool + reads BOTH `n` and `_n` same tick → needs the state-model refinement below |
| test_27_parallel_solving | GAP | inline `claim` composition (16-queens) + enum + `≠` — needs claim-inlining in the FSM transpiler |
| test_30_jit_gap_closures | GAP | String world field + `_world.X` (previous-world read) + div/mod + string-into-world — multiple sub-features |
| --- | --- | --- |
| test_06_shell_run        | OUT | `ShellRun "date"` → wall-clock-dependent output; not reproducible |
| test_07_time             | OUT | `Time`/`MonotonicTime` wall clock; engine deterministic-stubs it → differs from oracle |
| test_10_spawn            | OUT | process-spawn effect + spawned-FSM model — no spawn dispatch in the engine |
| test_11_frameclock       | OUT | async `FrameClock` event source — engine has no async sources |
| test_12_hostname         | OUT | FTI hostname (FFI) → host-dependent, non-reproducible |
| test_13_timer            | OUT | async `Timer` FTI — no async sources |
| test_14_stdin            | OUT | async stdin source — no async sources / external input |
| test_15_signal           | OUT | async `SIGINT` source — no async sources (transpiler rejects the reserved `signal_received` field rather than diverge) |
| test_16_sdl_red          | OUT | SDL FFI + display |
| test_17_sdl_triangle     | OUT | SDL/GL FFI + display |
| test_18_reflection       | OUT | `Program` AST reflection world-plugin — no reflection infra |
| test_24_sdl_mixer        | OUT | SDL_mixer FFI |
| test_31_symbolic_regression | OUT | output IS the SymbolicFunctionizer's discovered-formula string — strategy-dependent, not FSM behavior |
| test_32_llm_functionizer | OUT | LLM functionizer + stdin |
| test_33_satisfier        | OUT | satisfier-mode PRNG-drawn values; Z3 picks a different satisfying assignment → printed values differ |
| test_34_halts_within     | OUT | embedded `halts_within(F,N)` execution model — a different runtime tier than the SMT-LIB scheduler engine |
| test_35_run_fsm          | OUT | embedded `run(F,init)` execution model |
| test_36_sum_tree         | OUT | embedded run + recursive payload enums (`Tree`/`Stack`) |
| test_37_tree_walk        | OUT | embedded run + recursive payload enums (`NodeList`) + String labels |
| test_38_nested_effects   | OUT | embedded run with child-effect percolation to the parent |

## What this proves

The hybrid (greenfield engine + Evident→SMT-LIB transpiler) now runs the
**deterministic, single-tier, non-FFI FSM/logic corpus** end-to-end
byte-identical to the legacy runtime — scalars, Int/Bool/String arithmetic,
nullary AND payload-carrying enums, `match` (with payload binding), `last_results`
effect-result threading (`IntToStr`/`ParseInt`), intermediate Bool/String/Int
body vars, ternaries, `++` concat, Z3 string ops, multi-FSM programs over a shared
`world`, and constraint-solve loops (144 enum vars + 576 `≠` per tick). This is the
split-plan's go-forward thesis demonstrated on real examples: the SMT-LIB-input
boundary is a real runtime, not a toy. Started at 3 byte-identical (the
hybrid-integration convergence set); ended at 12.

## The honest boundaries

The 20 `OUT` rows are NOT transpiler laziness — they are real properties the
SMT-LIB scheduler engine does not (and should not pretend to) cover:

1. **Non-determinism** — wall clock (`Time`, `ShellRun "date"`), host identity
   (`hostname`), and strategy-dependent solver output (`symbolic_regression`'s
   discovered formula, `satisfier`'s PRNG-drawn values). No deterministic single
   trace exists to match.
2. **FFI / SDL / GL** — `LibCall`-backed effects, the SDL window/mixer demos, and
   the reflection `Program` plugin. The engine dispatches `Println`/`Exit`/
   `IntToStr`/`ParseInt` only.
3. **Async event sources** — FrameTimer / Stdin / Sigint / Timer. The engine ticks
   every FSM every tick with no blocking wait; there is no awaiter.
4. **The embedded `run(F,init)` / `halts_within` execution tier** (test_34–38).
   These run an FSM-as-a-value inside a driver FSM — a different runtime tier than
   the SMT-LIB multi-FSM *scheduler* this engine implements. Reproducing them needs
   the nested-FSM interpreter, not a transpiler shape.

## The 5 GAPs — feasible next steps (ranked)

1. **test_26** (and a cleaner model overall): replace the current "engine prev ==
   oracle current, rename the scalar var" trick with a synthetic first-tick Bool
   state var (`_first` init true → false) and the natural `next`/`prev` mapping.
   That lets `is_first_tick` appear in any derived var and lets a tick read both `n`
   and `_n`. Re-verify test_19/20/29 stay byte-identical.
2. **test_22 / test_25** — record-typed state and `Set(Int)` intermediates: record
   transpile (per-field decls, already partly present for `type World`).
3. **test_27** — inline `claim` composition inside an FSM body.
4. **test_30** — String world fields + `_world.X` previous-world reads + div/mod.

`run-matrix.sh` / `crosscheck.sh` / `tests/convergence.rs` cover the convergence
proof; this file is the corpus-wide ledger.
