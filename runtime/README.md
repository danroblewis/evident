# `runtime/` — Evident, Rust implementation

The Rust runtime is the only implementation of Evident. The language is
defined by what this crate parses, translates to Z3, and executes.

What ships:
- A constraint-solver façade — `EvidentRuntime` with `load_file`, `query`,
  `query_cached`, `sample` — backed by Z3.
- A multi-FSM scheduler (`effect_loop`) that runs `evident effect-run …`
  programs.
- A JIT functionizer (`functionize`) that compiles extracted `Z3Program`s
  to native code via Cranelift. JIT misses fall through to a full Z3 solve.
- FFI / FTI bridges (`ffi.rs`, `fti.rs`, `event_sources/`) so programs can
  reach SDL, stdin, signals, frame timers, the wall clock, etc.
- A CLI binary (`main.rs` + `commands/`) exposing `query`, `sample`,
  `check`, `test`, `effect-run`, `lint`, `desugar`, `infer-types`.

## Quick start

```sh
cargo build --release                              # build the crate + binary
./test.sh                                          # run all tests (~50s)
./runtime/target/release/evident effect-run X.ev   # run an effect program
```

Tests: `./test.sh` from the repo root runs Rust units + integration
tests + Python conformance. `./test.sh --rust-only`, `--conformance`, or
`--examples` for subsets.

Z3 is required. On macOS: `brew install z3`.

## Source layout

Single-concern modules under `runtime/src/`. The full "want to change X →
edit file Y" table lives in [`../CLAUDE.md`](../CLAUDE.md#source-layout-which-file-owns-what).
Top-level summary:

| Module | Purpose |
|---|---|
| `core/`          | Shared data types + traits (Evident AST, `Value`, `Z3Program`, `Functionizer` trait, `QueryResult`, …). Imported by everything. No orchestration logic. |
| `runtime/`       | `EvidentRuntime`: load, query, sample, scheduler-facing API |
| `effect_loop/`   | Multi-FSM scheduler — `run` and `run_with_ctx` |
| `translate/`     | Evident AST → Z3 ASTs; build solvers; extract models |
| `functionize/`   | Functionizer implementations (currently: Cranelift JIT) |
| `event_sources/` | Async wake plugins (FrameTimer, Stdin, Sigint, FileWatcher, …) |
| `commands/`      | Per-CLI-subcommand entry points |
| `effect_dispatch.rs` | `Effect → IO` (Println, LibCall, ParseInt, …) |
| `subscriptions.rs`   | Static read/write-set inference per claim |
| `z3_eval.rs`     | Extract a `Z3Program` from a simplified Z3 AST |
| `ffi.rs`, `fti.rs`   | libffi marshaling + typed-resource bridges |
| `parser.rs`, `lexer.rs`, `pretty.rs` | Front end |

Run `scripts/rust-size.py --per-file` for the current line-count table.
Target: ≤ 500 lines per file.

## Architecture

Two layers: a **core** of shared data types + traits with no orchestration
logic, and an **application stack** of subsystems built on top of it.
Every application module depends on `core::*`; those edges aren't drawn
because they're universal.

### Core (`runtime/src/core/`) — the vocabulary

Data types and traits. No behavior beyond what the types themselves
need (constructors, simple accessors). Imported by everything else.

```mermaid
graph LR
    subgraph core[core/]
        ast[ast.rs<br/>Evident AST<br/>Expr · BodyItem · SchemaDecl<br/>Effect · EffectResult · Pins]
        value[value.rs<br/>Value · EvalResult]
        z3t[z3_types.rs<br/>EnumRegistry · CachedSchema<br/>Var · FieldKind · DatatypeRegistry]
        z3p[z3_program.rs<br/>Z3Program · Z3Step · GuardedBody]
        api[api.rs<br/>QueryResult · RuntimeError]
        fzt[functionizer.rs<br/>Functionizer · CompiledFunction]
    end
```

(No edges — each file is independent. The whole module is a leaf.)

### Application stack — orchestration

Each module depends on `core::*` (implicit, not drawn) plus the modules
below it. Edges point from importer → imported.

```mermaid
graph TD
    main[main.rs]
    cmds[commands/]
    eloop[effect_loop/]
    rt[runtime/]
    fz[functionize/]
    tr[translate/]
    z3e[z3_eval.rs]
    edisp[effect_dispatch.rs]
    esrc[event_sources/]
    subs[subscriptions.rs]
    fti[fti.rs]
    ffi[ffi.rs]
    parser[parser.rs]
    lexer[lexer.rs]
    dec[decompose.rs]
    z3p[z3_profile.rs]
    vb[value_builders.rs]

    main --> cmds
    cmds --> rt
    cmds --> eloop

    eloop --> rt
    eloop --> edisp
    eloop --> esrc
    eloop --> subs

    rt --> parser
    rt --> tr
    rt --> fz
    rt --> z3e
    rt --> dec

    fz --> z3e
    fz --> vb

    esrc --> edisp
    esrc --> fti

    edisp --> ffi
    fti --> ffi

    tr --> z3p

    parser --> lexer
```

Reading order if you're new: `core/` (the vocabulary) → `parser.rs` →
`translate/` (the inline → eval pipeline) → `z3_eval.rs` (program
extraction) → `functionize/` (program → native code) → `runtime/` (the
façade) → `effect_loop/` (how the scheduler drives it).

## `evident effect-run` flow

What happens when you type `evident effect-run examples/test_21_mario/main.ev`:

```mermaid
flowchart TD
    CLI[evident effect-run prog.ev]
    Run[commands/effect_run::cmd_effect_run]
    Load[rt.load_file stdlib/runtime.ev + prog.ev]
    Loop[effect_loop::run]
    Detect[detect FSMs: claims with state pair + EffectList + ResultList]
    Tick{Tick scheduler}
    Sub[subscriptions: which FSMs have a changed input?]
    Block[block on async SchedulerEvent channel<br/>FrameTimer / Stdin / Sigint]
    Q[runtime::query_with_pins_and_given<br/>per scheduled FSM]
    TFZ[try_functionize_z3]
    Cache{fn_cache hit?}
    Compiled[CompiledFunction::call → native code]
    Extract[extract Z3Program from simplified body]
    JIT[functionizer.compile<br/>= CraneliftFunctionizer::compile]
    JitOK{JIT compiled?}
    Slow[crate::translate::evaluate<br/>full Z3 solve]
    Out[state_next + Seq Effect emitted]
    Disp[effect_dispatch::dispatch_all<br/>Println / LibCall / SDL / ParseInt / …]
    World[update world snapshot]
    Halt{any FSM emitted<br/>Effect::Exit, or no FSM<br/>scheduled this tick?}
    Done[LoopResult<br/>→ process exit code]

    CLI --> Run --> Load --> Loop --> Detect --> Tick
    Tick --> Sub
    Sub -- nothing ready --> Block --> Tick
    Sub -- ≥1 FSM ready --> Q
    Q --> TFZ --> Cache
    Cache -- hit --> Compiled
    Cache -- miss --> Extract --> JIT --> JitOK
    JitOK -- yes --> Compiled
    JitOK -- no --> Slow
    Compiled --> Out
    Slow --> Out
    Out --> Disp --> World --> Halt
    Halt -- no --> Tick
    Halt -- yes --> Done
```

Key files for each step (so you can read the code in order):

| Step | File:fn |
|---|---|
| CLI dispatch | `runtime/src/commands/effect_run.rs:cmd_effect_run` |
| Load + import resolution | `runtime/src/runtime/load.rs` |
| FSM detection | `runtime/src/effect_loop/fsm.rs:all_fsms` |
| Scheduler entry | `runtime/src/effect_loop/mod.rs:run_with_ctx` |
| Multi-FSM tick loop | `runtime/src/effect_loop/multi_fsm.rs:run_multi_fsm` |
| Subscription wake set | `runtime/src/subscriptions.rs:world_access_sets` |
| Per-FSM query | `runtime/src/runtime/scheduler_api.rs:query_with_pins_and_given` |
| Functionize / JIT path | `runtime/src/runtime/query.rs:try_functionize_z3` |
| JIT codegen | `runtime/src/functionize/cranelift.rs:compile_program` |
| Compiled-fn dispatch | `runtime/src/functionize/cranelift.rs:JitProgram::call` |
| Slow-path Z3 solve | `runtime/src/translate/eval/mod.rs:evaluate` |
| Effect dispatch | `runtime/src/effect_dispatch.rs:dispatch_all` |
| Async wake sources | `runtime/src/event_sources/` |

## Functionizer strategy

The runtime calls a `Functionizer` trait (`functionize/mod.rs`); the
default impl is `CraneliftFunctionizer` (`functionize/cranelift.rs`).
To swap in a different strategy:

```rust
let rt = EvidentRuntime::with_functionizer(Box::new(MyStrategy));
```

There is exactly **one** `impl Functionizer` in the tree today. JIT
misses fall through to a full Z3 solve via `translate::evaluate` — no
intermediate fallback layers.

## Environment variables (debugging / tuning)

| Var | Effect |
|---|---|
| `EVIDENT_FUNCTIONIZE=0`        | Disable functionizer (force slow-path Z3) |
| `EVIDENT_FUNCTIONIZE_STATS=1`  | Print `[fz/stats]` summary on exit |
| `EVIDENT_FUNCTIONIZE_TRACE=1`  | Per-call trace of fz hits/misses |
| `EVIDENT_LOOP_TIMING=1`        | Per-FSM timing breakdown |
| `EVIDENT_DISPATCH_TIMING=1`    | Per-effect dispatch timing |
| `EVIDENT_LENIENT=1`            | Demote dropped-constraint errors to warnings |
| `EVIDENT_TACTICS=…`            | Override Z3 tactic chain (`solve-eqs`, `simplify`, `standard`, `aggressive`, …) |
| `EVIDENT_Z3_ARITH_SOLVER=N`    | Force `smt.arith.solver=N` (skips autotuner) |
| `EVIDENT_Z3_AUTOTUNE=0`        | Disable per-claim autotuner pricing |
| `EVIDENT_SCHEDULER=legacy`     | Use the pre-subscription "tick every FSM" scheduler |
| `EVIDENT_TICK_MS=N`            | FrameTimer rate (multi-FSM scheduler wake interval) |
| `EVIDENT_JIT_TRACE=1`          | Per-AST-node trace from the Cranelift codegen |
| `EVIDENT_JIT_CALL_TRACE=1`     | Print every JIT call result |
| `EVIDENT_PROFILE_Z3=1`         | Z3 statistics summary on exit |

## CLI

```sh
evident query       <files…> <schema> [--given k=v …] [--json]
evident sample      <files…> <schema> [-n N] [--given k=v …] [--json]
evident check       <files…>
evident test        [path]            # walks for test_*.ev, runs sat_/unsat_ claims
evident effect-run  <file>            # run an effect-driven program
evident lint        <file>
evident desugar     <file>            # report self-hosted desugar rewrites
evident infer-types <file>            # report self-hosted type inferences
```

Output:
- `query` SAT  → `KEY=VALUE` lines (sorted), exit 0
- `query` UNSAT → `UNSAT`, exit 1
- `--json` → `{"satisfied": …, "bindings": {…}}`
- `check` → `SAT|UNSAT|ERROR  <name>` per schema; exit 1 if any UNSAT
- `test` → `PASS|FAIL  <name>` per claim, plus a final summary
- `effect-run` → process exit code from `Effect::Exit(N)`, else 0 on clean halt, 1 on max-steps

## Where to read first

1. [`../CLAUDE.md`](../CLAUDE.md) — language conventions and the
   source-layout lookup table.
2. [`../docs/design/schema-interface.md`](../docs/design/schema-interface.md)
   — the unifying framing of what an Evident model IS.
3. [`../docs/design/multi-fsm.md`](../docs/design/multi-fsm.md) — the
   scheduler model `effect_loop/` implements.
4. [`../docs/design/minimal-runtime.md`](../docs/design/minimal-runtime.md)
   — architectural goals (~11K Rust target, FFI-first).
5. `runtime/src/lib.rs` — module manifest; everything starts there.
