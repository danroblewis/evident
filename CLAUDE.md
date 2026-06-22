# Evident — Project Invariants

## What This Is

Evident is a constraint programming language where programs are collections of
constraints over sets, and a Z3 SMT solver finds satisfying assignments.  The
central abstraction is `schema`: a named set defined by membership conditions.
Querying a schema asks whether a satisfying assignment exists.

## The core, and the cruft rule — read `docs/design/core.md` FIRST

[`docs/design/core.md`](docs/design/core.md) is the yardstick. Before adding OR
removing anything, judge it against the core there: **is this part of the core
pipeline, does it directly protect it, or is it on the roadmap — or is it
incidental complexity we dragged along?** If incidental, it does not belong in
the core — "useful" is not a defense, half-features are worse than absent ones,
and we add things (perf measurement, error handling, conveniences) only when a
real need demands them. When you propose a removal or an addition, state the
one-line judgment against `core.md`; don't decide on caller-count or line-count
alone.

## Navigate code with Semfora FIRST — do not pull whole files blind

This repo is indexed by **Semfora**, an MCP server (`semfora-engine serve`) plus
an FS-monitor daemon, baked into `Dockerfile.dev` and started on boot. It exposes
symbol search, callpath tracing, and surgical source reads. It is the **first**
tool you reach for to find, understand, or assess the impact of code — before
`Read`, and instead of shelling `grep`/`cat`/`rg`/`sed` over source files.

**Required workflow when working on code:**
  * **Find** a symbol or concept → `mcp__semfora__search` (hybrid symbol+semantic).
    Do NOT grep-then-Read files to locate things.
  * **Before editing ANY function** → `mcp__semfora__get_callers` on it to see the
    blast radius. Editing a function without first checking its callers is the
    exact failure mode this is here to prevent.
  * **Read** a specific function or any large file → `mcp__semfora__get_source`
    (by symbol hash, or file+lines) — a surgical read, never the whole file. For
    files larger than ~500 lines, NEVER use the `Read` tool; use
    `get_source` / `get_symbol`.
  * **Cleanup / minimization** (a standing goal in this repo) →
    `mcp__semfora__dead_code_audit` + `mcp__semfora__find_duplicates` before
    proposing any removal.
  * **Reviewing a diff or changes** → `mcp__semfora__analyze_diff`.

The `Read` tool is reserved for exactly two cases: (1) the file you are *about to
`Edit`* (the harness requires a prior `Read`), and (2) small non-code assets
(config, `.toml`, docs). Pulling source files to "look around," or shelling
`grep`/`cat` over `.rs`/`.ev` files to explore, is the anti-pattern Semfora
replaces. Try Semfora first, every time; only fall back to `search`/`Read` if a
Semfora query genuinely returns nothing useful. The daemon keeps the index fresh;
`search` auto-refreshes it if it looks stale.

## Run `./test.sh` before declaring work done

There is one test command: **`./test.sh` from the repo root**.

It builds the runtime in release mode, then runs `cargo test --release` (Rust
units + integration tests + the demo driver that runs every
`examples/test_*.ev` end-to-end, including the SDL demos on the Xvfb display).
All phases must pass; the script exits non-zero if any phase fails.

The full run is **~10 seconds** when the binary is already built.

When to run it:
  * After any change that touches `runtime/`, `stdlib/`, `examples/`,
    or `tests/`.
  * Before the end of a multi-step task — even if you ran a subset of
    tests during the work, run the full thing once at the end.
  * If `./test.sh` fails, fix the failures before declaring done. Don't
    add `xfail` markers as a TODO; either fix the code or, if it
    surfaces a runtime gap, file an entry in
    `examples/COUNTEREXAMPLES.md` and delete the test.

Iteration-only flags:
  * `./test.sh --examples` — phases 1–2 PLUS run every demo in
    `examples/` end-to-end via the binary, capturing
    screenshots for visual demos.
  * `./test.sh --examples-only` — just the examples runner;
    assumes the binary is already built.

The default — no flags — is what you should run before claiming work
is done.

### Visual verification of `--examples`

When `--examples` runs, it iterates every `examples/test_*.ev`:
- Non-visual demos run with a timeout, asserting clean exit.
- Visual demos (anything importing `packages/sdl/`) get spawned,
  given ~2s to draw, screenshotted to `/tmp/evident-screenshots/`,
  then killed.

The exit-code check covers correctness for non-visual demos but
**says nothing about whether visual demos render correctly** —
they could exit cleanly while showing a black window. The
agentic loop closes that gap:

  1. Run `./test.sh --examples`.
  2. List `/tmp/evident-screenshots/` to see which demos captured.
  3. For each PNG, use the Read tool (it accepts image paths and
     renders them inline). Visually verify the screenshot matches
     what the demo's docstring claims it should show — red window
     for `test_16_sdl_red`, RGB triangle for `test_17_sdl_triangle`,
     etc.
  4. If a demo renders something different from its docstring,
     either fix the demo, fix the runtime, or document the gap in
     `examples/COUNTEREXAMPLES.md`.

This is the only way visual regressions get caught — an agent
running `./test.sh --examples` and Reading the PNGs is functionally
the visual-test harness. We don't have a pixel-diff CI yet.

## Task & concern tracking for the web-IDE loop — `python3 ide/task.py`

The web-IDE goal loop is tracked in `ide/tasks.json` through a CLI (run from the repo
root). There are two object types: **tasks** (units of work) and **concerns** (worries
the critics or Iris raise). The whole loop runs through this ledger — use it every round.

**Your loop, each round:**
1. `python3 ide/task.py summary` and `… list --open` — see what's left. The goal loop is
   done when every task is **closed** (not just done).
2. `python3 ide/task.py list --concerns --open` — read what the critics are worried about.
   For each open concern, add a task to resolve it:
   `python3 ide/task.py add "<the fix>" --from-concern <ID>`. **You may NOT clear a
   concern** — only its author (the critic/Iris who raised it) clears it, once satisfied.
3. Work a task: `python3 ide/task.py start <ID>` → do it → `python3 ide/task.py done <ID>`.
4. **`done` does NOT close a task.** A task closes only with your `done` PLUS an `approve`
   from ALL THREE critics (`ide-critic`, `ide-critic-newcomer`, `ide-critic-expert`). So
   after a build you MUST run the critics; they review your done-tasks and either `approve`
   or `reopen` them (logging a concern saying why). A reopened task resets its approvals.

The critic and Iris subagents use the SAME CLI (their prompts instruct them): they
`add` tasks and `concern`s, `approve`/`reopen` your done-tasks, and `clear-concern` their
own concerns. Only the three critics approve/reopen; only an author clears their concern;
you (the worker) never approve or clear. `ide/task.py --help` lists every command.

### Reorganize freely — don't pile onto the nearest file

When you change `ide/` or `viz/` code, **move code to where it belongs** rather than appending to
whatever file you're already in. If a file has outgrown its concern, SPLIT it (new module along the
seam, fix the imports — check `get_callers` first); if a function is too long, break it along its
phases; if you've copy-pasted a shape, extract a helper. A behavior-preserving reorganization that
keeps `./test.sh` green is always welcome — you don't need permission to tidy the house.

Two things keep this honest: a **lint ratchet** (`ide/lint.py`, wired into `./test.sh` Phase 2.5 —
fails when `ide/`+`viz/` gain NEW file-length / function-length / free-function / coupling violations
over `ide/.lint-baseline`; `--write-baseline` after a real reduction), and the **`ide-architect`
agent** (Dijkstra), who surveys structure, does the safe splits himself, and files `--tag refactor`
tasks + concerns for the larger ones. Spawn him when the IDE code starts to feel heavy.

## Where to read first

Before writing code in this repo, check whether one of these guides covers
your task:

| If you're … | Read |
|---|---|
| Writing a new program (any program) | [`examples/`](examples/) — copy the closest existing demo's shape |
| Looking for the punch list of known runtime gaps | [`examples/COUNTEREXAMPLES.md`](examples/COUNTEREXAMPLES.md) |
| Writing or debugging a program that uses `evident effect-run` | [`docs/guide/effect-state-machines.md`](docs/guide/effect-state-machines.md) |
| Writing or extending an FFI wrapper library (`packages/sdl/`, `packages/gl/`, `stdlib/shell.ev`, …) | [`docs/guide/ffi-bindings.md`](docs/guide/ffi-bindings.md) |
| Understanding what an Evident model IS (the unifying framing) | [`docs/design/schema-interface.md`](docs/design/schema-interface.md) |
| Trying to understand the architectural goals (~11K Rust target, FFI-first) | [`docs/design/minimal-runtime.md`](docs/design/minimal-runtime.md) |
| Designing the FFI primitive itself or extending it | [`docs/design/ffi-design.md`](docs/design/ffi-design.md) |
| Planning what to add to FFI / OS coverage (reads, writes, alloc, callbacks, posix) | [`docs/design/ffi-os-evolution.md`](docs/design/ffi-os-evolution.md) |
| Looking for plan files for the larger refactor | [`docs/plans/README.md`](docs/plans/README.md) |

The two `docs/guide/*` docs were written specifically to spare future-you
the painful debug sessions that produced them. If you're about to write a
state machine or an FFI binding, **read those first**.

## Conventions for `examples/` (this repo's test/example set)

These rules govern files we write into `examples/`. They
are NOT a property of the Evident language — a downstream user
writing their own Evident program is not bound by them. Inside
this repo, `examples/` is our canonical test set: every
file there doubles as a worked example AND an integration
test, so we hold them to a strict shape.

### 1. Demo files are integration tests

Each file in `examples/` is named `test_NN_<name>.ev`
and contains both:

  * The FSM program (a `claim` with `_var`-carried state +
    `last_results ∈ ResultList` + `effects ∈ EffectList`),
    which the trampoline ticks.
  * Inline `claim sat_*` / `claim unsat_*` static tests that
    pin state/inputs and assert on the FSM's response.

Two test runners cover both halves:

  * `evident test examples/` — discovers `test_*.ev`
    files, runs every `sat_*` / `unsat_*` claim.
  * `cargo test --test demos` (in `runtime/`) — runs
    each demo end-to-end via the binary, asserts on exit
    code and stdout substring. The `EXPECTATIONS` table in
    `runtime/tests/demos.rs` is the contract.

When adding a demo: drop the file in `examples/`, add a
row to `EXPECTATIONS`. Both runners stay green.

(The trampoline skips claims named `sat_*` / `unsat_*` when
discovering the FSM — they have the FSM shape because they pin
`state` / `effects` to assert properties, but they should never
run as the FSM. This applies everywhere, not just to demo files.)

### 2. Demo files MUST NOT contain raw FFI calls

In any `examples/*.ev` file (and any other example we author),
`LibCall` / `FFICall` / `FFIOpen` / `FFILookup` are forbidden. Demos reach C code by either:

  * Calling **named claims** from `stdlib/` that wrap the FFI
    behind a typed interface. Example: `sdl_pump_events(out)` —
    not `out = LibCall("/opt/homebrew/lib/libSDL2.dylib", …)`.
  * Declaring **FTI typed resources** as parameters or body
    items (`win ∈ SDL_Window (title ↦ "X", …)`) and letting
    the runtime's bridge install own the C-side lifecycle.

If a demo needs a C function that no stdlib helper covers:
**add the helper to stdlib** (`stdlib/<library>/...ev`) first,
then call it from the demo. A demo file containing
`LibCall(...)` or a hardcoded library path like
`"/opt/homebrew/lib/libSDL2.dylib"` is a code-review blocker —
move it to stdlib.

The COUNTEREXAMPLES file lists what the runtime can't yet do
(e.g. SDL+GL render-via-dispatch). Don't work around those by
reaching into raw FFI from a demo; either fix the runtime, add
a stdlib wrapper, or document the limit.

(Outside `examples/` — your own application code, ad-hoc
exploration, etc. — these rules don't apply. They're a quality
bar for the canonical test set.)

## Language Definitions

The Rust runtime under `runtime/` is the only implementation. The
language is defined by the lexer + parser + AST + translator that
ship with it.

| Thing | Where defined |
|---|---|
| Lexer (Unicode operators → tokens) | `runtime/src/lexer.rs` |
| Parser (recursive-descent) | `runtime/src/parser/` (`mod.rs` decls, `exprs.rs` expression chain) |
| AST node types | `runtime/src/core/ast.rs` |
| Shared types (Value, Z3Program, registries, …) | `runtime/src/core/types.rs` |
| AST → Z3 encoder | `runtime/src/encode/` |
| Z3 functionizer + JIT | `runtime/src/functionize/` (Cranelift impl) + `runtime/src/functionize/extract_program.rs` (extractor) |
| Effect dispatch | `runtime/src/ffi.rs` |
| Run loop (ticks the single FSM) | `runtime/src/trampoline.rs` |
| FTI bridges | `runtime/src/ffi.rs` (`is_shimmed_stdlib`) |
| Stdlib (Evident) | `stdlib/` |
| Design docs | `docs/design/` |
| Worked examples + integration tests | `examples/` |

## Runtime Architecture

The runtime is a pipeline. Each stage is a separate module under
`runtime/src/`:

```
source text
  → lexer.rs              Unicode operators + word-keywords → tokens
  → parser/               Recursive-descent parser → AST (core/ast.rs)
  → encode/            AST → Z3 sorts + constraints; per-claim inline
  → functionize/extract_program.rs            Simplified Z3 AST → Z3Program (the IR)
  → functionize/          Z3Program → callable function (Cranelift JIT)
  → runtime/              EvidentRuntime: top-level API (load_file, query)
  → trampoline.rs        Run loop: ticks the single FSM (the executor)
  → ffi.rs               Effect → IO (Println, LibCall, ParseInt, …)
```

Supporting modules:
- `ffi.rs` — libffi marshaling, handle registry
- `functionize/extract_program.rs` — extract a `Z3Program` from a simplified Z3 AST
- `main.rs` — the CLI binary: `test` / `effect-run` entry points + output

## Source layout: which file owns what

Files under `runtime/src/` are organized by single-concern modules,
typically ≤ 500 lines per file. When you need to change a thing,
here's where to start.

### Top-level modules

| Module | What lives here |
|---|---|
| `core/`        | Shared data types. No orchestration logic. Imported by everything else. |
| `runtime/`     | `EvidentRuntime`: load, query, sample, run-loop-facing API |
| `trampoline.rs` | Run loop — `run` and `run_with_ctx`, FSM discovery, install bridge, per-tick loop, effect ordering |
| `encode/`   | Evident AST → Z3 ASTs; build solvers; extract models |
| `functionize/` | Functionizer implementations (currently: Cranelift JIT) |
| `main.rs` | The CLI binary: `test` / `effect-run` entry points + test-report output |
| `ffi.rs` | Effect → IO (Println, LibCall, ParseInt, …) |
| `functionize/extract_program.rs`   | Extract a `Z3Program` from a simplified Z3 AST |
| `ffi.rs`       | libffi marshaling + handle registry; FTI shimmed-stdlib check |
| `parser/`, `lexer.rs` | Front end (AST Display rendering lives in `core/ast.rs`) |

### Inside `core/`

The vocabulary of the codebase. When you reach for a shared data type
or trait, it lives here. **Don't put orchestration logic in `core/`.**
If you find yourself adding a function that calls `rt.query(…)`, that's
not a core thing — it belongs in `runtime/` or higher.

| File | What's in it |
|---|---|
| `core/ast.rs`          | Evident AST — `Expr`, `BodyItem`, `SchemaDecl`, `Effect`, `EffectResult`, `Pins`, `BinOp`, `Program`, `Keyword` |
| `core/types.rs`        | Everything else shared: `Value`/`EvalResult`; the Z3-side `EnumRegistry`, `Var`, `FieldKind`, `SeqFieldElem`, `DatatypeRegistry`, `CompiledModel`; the `Z3Program`/`Z3Step`/`GuardedBranch`/`GuardedBody` IR; `QueryResult`/`RuntimeError`; the `parse_seq_type`/`internal_cons_helper_name` Seq helpers |

External callers can use `evident_runtime::{Value, QueryResult, RuntimeError, ast}` (re-exported from `core` at `lib.rs`). Internal code imports from `crate::core::*` directly.

### Inside `session/`

| Want to … | Edit |
|---|---|
| Add a new public query method | `session/query.rs` |
| Change how a schema gets parsed/loaded, or import resolution | `session/mod.rs` (load + run-loop-facing query + UnionFind) |
| Add/change a lowering pass (seq-concat desugar, Δ desugar, FSM/type-inference injection) | `encode/lower.rs` |
| Touch enum → Z3 Datatype registration | `session/register_enums.rs` |

### Inside `trampoline.rs` (single file, sectioned)

| Want to … | Section |
|---|---|
| Change how FSMs are discovered | FSM discovery + shape (`resolve_fsm`, `single_fsm`) |
| Change effect ordering / toposort | effect collection + ordering (`collect_dispatchable_effects`, `topo_sort_with_random_tiebreak`) |
| Touch the per-tick run loop | per-tick run loop (`run_loop`) |
| Adjust state seeding / encoding | `seed_state`, `encode_state_value` |
| Touch the declarative install bridge | `run_declarative_install` |

### Inside `encode/eval.rs` (single file, sectioned)

| Want to … | Section |
|---|---|
| Change the default tactic chain or solver tuning | solver tuning + shared helpers (`make_tuned_solver`) |
| Touch the one-shot vs cached path | `evaluate` / `build_cache` + `run_cached` |
| Touch the extra-assertions (pins) variant | `evaluate_with_extra_assertions` |
| Decode a new enum/composite shape from a Z3 model | model → Value decoders (`extract_enum_value`, `extract_seq_enum`) |

### Inside `functionize/`

| Want to … | Edit |
|---|---|
| Touch the JIT program repr / C-helper runtime / Functionizer entry | `functionize/cranelift/mod.rs` |
| Change Cranelift codegen (compile_program + emit_* walkers) | `functionize/cranelift/codegen.rs` |

### Rules of thumb

- **One file = one concern.** If you're adding > ~200 lines to a file in this layout, ask whether it's actually a new concern.
- **Public re-exports from `mod.rs`.** `crate::session::EvidentRuntime` works whether the type is defined in `session/mod.rs` or somewhere under it.
- **Sibling visibility: `pub(super)`** for cross-file helpers inside a directory module.
- **Tests next to the code.** `#[cfg(test)] mod tests { … }` at the bottom of the file under test.
- **`scripts/rust-size.py`** lists files by length — run it when you suspect a file is overdue for a split.

## The run loop (single FSM)

For programs run via `evident effect-run`, the trampoline in
`runtime/src/trampoline.rs` discovers the one top-level claim matching
the FSM shape (`_var`-carried state + EffectList + ResultList) and ticks
it. Each tick reads the previous tick's state (exposed as `_var`), solves
the FSM claim for this tick's state +
its effects, dispatches those effects as real IO, writes the new state
back, and bounces again.

**Halt is implicit.** No `Done`/`Halt` name convention, no fixpoint
heuristic. The program halts when a tick changes nothing (nothing more
can happen) or when the FSM emits `Effect::Exit(code)`.

**`Effect::Exit(code)` is graceful** — it sets `exit_requested` on the
dispatch context. The runtime dispatches all of the current tick's
effects first (so final cleanup writes / logs run), then halts at
end-of-tick with the requested code. `LoopResult::exit_code` propagates
to the CLI as the process exit code.

## Idiomatic Evident — drop annotations the inference can recover

The runtime infers types from RHS expressions, claim signatures,
and constructor calls. **When generating Evident code, prefer the
shorter form**; spelling out `∈ Type` where it would be inferred
is noise that bloats the source.

### Chained-membership and dropped annotations

Prefer one line over two:

```evident
-- Don't:
result ∈ Int
result = x + 1

-- Do (chained-membership):
result ∈ Int = x + 1
```

When the RHS makes the type determinable, drop the annotation
entirely (the lhs-eq inference recovers it):

```evident
-- All of these have their type inferred:
on_ground = (_pos.y ≥ 400)                  -- Bool from comparison
walk_vx = (key_left > 0 ? 0 - 5 : 0)        -- Int from ternary arms
sky = Color(80, 160, 220, 255)              -- Color from ctor
eff = LibCall("...", "...", "...", ⟨⟩)      -- Effect from variant
target = _state.pos                          -- IVec2 from field type
m_str = match last_results[0]                -- Int from arm bodies
    IntResult(n) ⇒ n
    _           ⇒ 0
```

Inference covers ternary arms, match arm bodies, binary ops
(comparisons / logical → Bool; arithmetic → operand type; `++`
→ String), constructor calls (`Color`, `IVec2`, `LibCall`, etc.),
field access on declared records (chains through schema bodies),
and claim-call args (fresh names used multiply get types from
the called claim's signature).

What stays explicit:
- **Top-level primitive literals** — `x = 5` doesn't auto-infer
  at load time. Use `x ∈ Int = 5`.
- **Record arithmetic with no anchoring side** — `tent ∈ IVec2 =
  _pos + grav_vel` (inference doesn't yet propagate record type
  through `+`).
- **Type definitions** — `type GameState` body needs annotations.

### Record-typed FSM state: `_var` / `var`

Record state carries tick-to-tick exactly like a scalar — there is no
"next snapshot" variable and no special "world" concept. `_var.field`
reads the previous tick's value; `var.field` writes this tick's value.

```evident
type GameState
    pos ∈ IVec2

-- Don't (the old `*_next` write-snapshot pair):
fsm game(state ∈ GameState, state_next ∈ GameState)
    state_next.pos = state.pos + ...         -- the abandoned writer pattern

-- Do (`_var` time-shift, identical to scalars):
fsm game(state ∈ GameState)
    state.pos = _state.pos + ...             -- _state.X read prev, state.X = write
```

`_state.X` reads the previous tick's value; `state.X = ...` writes this
tick's value, and the runtime carries `state → _state` per-field across
ticks. On the first tick there is no previous, so `is_first_tick` is true
and the program seeds explicitly (`is_first_tick ⇒ state.pos = …`). This
is the ONE state-carrying mechanism: scalars, records, and enums all use
it. There is no `state_next`; a name carried as `_var`/`var` is the whole
story.

### Subclaim dispatch over receiver-prefix

When a type has subclaims, prefer `recv.subclaim(args)` over
threading the receiver explicitly:

```evident
-- Don't:
set_draw_color(win.renderer, Color(220, 40, 40, 255), eff)

-- Do (set_draw_color is a subclaim of SDL_Window):
win.set_draw_color((220, 40, 40, 255), eff)
```

The subclaim body uses the receiver's fields by bare name
(field-rebinding at invocation). The runtime resolves
`renderer` to `win.renderer`.

### Tuple-as-record coercion in claim arg slots

When a claim's slot has a record type, pass a bare tuple and
the runtime constructs the record literal:

```evident
-- Don't:
win.set_draw_color(Color(220, 40, 40, 255), eff)
win.render_fill_rect(IVec2(pos.x, pos.y), IVec2(32, 32), eff)

-- Do:
win.set_draw_color((220, 40, 40, 255), eff)
win.render_fill_rect((pos.x, pos.y), (32, 32), eff)
```

### Claim-arg type inference for fresh output names

When you pass a fresh identifier as a claim arg AND reference
it elsewhere, the runtime infers its type from the called
claim's param signature:

```evident
-- Don't:
sky_eff ∈ Effect
set_draw_color(win.renderer, Color(...), sky_eff)
effects = ⟨sky_eff, ...⟩

-- Do (sky_eff inferred as Effect from set_draw_color's `out` slot,
-- since it appears in both the call and the effects list):
set_draw_color(win.renderer, Color(...), sky_eff)
effects = ⟨sky_eff, ...⟩
```

Typo defense: only fires when the name has ≥ 2 uses. A
single-use fresh name stays undeclared so translation fails
loudly on typos.

### `_var` for previous-tick reads

Inside an fsm body, `_var` is the previous tick's value of
`var`. Works for primitive Ints, records (per-field, e.g.
`_state.pos.x`), and enums (whole-value). `is_first_tick ∈ Bool`
auto-injects when any `_var` is referenced.

```evident
fsm counter
    count ∈ Int = (is_first_tick ? 0 : _count + 1)
```

### Prefer `Δ` for the carried *delta*

When a carried var *changes by an amount each tick*, write the change as a
forward-difference `Δ`, not `_var + step` arithmetic. `Δcount` desugars to
`count − _count`, so the source states the difference equation directly — and
the sign is the whole feature (`Δx = -1` falls, `+1` climbs). This matches
Evident's relational framing: you state the relation between ticks, not a
recomputation of next-from-prev.

```evident
-- Prefer:
fsm pick
    count ∈ Int
    1 ≤ step ∈ Int ≤ 3
    is_first_tick  ⇒ count = 0
    ¬is_first_tick ⇒ Δcount = step

-- Over:
fsm pick
    count = (is_first_tick ? 0 : _count + step)
    1 ≤ step ∈ Int ≤ 3
```

**One rule, and it bites silently: declarations live *outside* the guards.** A
`name ∈ T` or chained-membership decl (`1 ≤ step ∈ Int ≤ 3`) placed *inside* a `⇒`
block is a parse error; and a carried var that is never declared at all has its
constraints **silently dropped** as an unbound identifier (no error). Declare every
carried var on its own line first.

The guard *body* is flexible — a single-line `⇒ Δcount = step`, an **indented
multi-line `⇒` block grouping several deltas under one guard**, or a paren-wrapped
conjunction all work and all keep 0 dropped:

```evident
fsm accumulate
    i   ∈ Int                         -- declare carried vars first, outside the guards
    sum ∈ Int
    is_first_tick ⇒
        i = 0
        sum = 0
    ¬is_first_tick ⇒                  -- one guard, several deltas, indented block
        Δi   = (_i < 5 ? 1 : 0)
        Δsum = (_i < 5 ? _i : 0)
```

`Δ` is for *deltas*. A value that's a fresh function of other vars each tick
(`state = (x ≤ 7 ? Landed : Falling)`) stays a plain equation — don't force `Δ`
on it. Underscore *reads* (`_i` inside a condition) remain fine; the preference is
about the carry, not a ban on `_var`. Canonical example:
`examples/test_23_difference.ev`.

## Keyword Conventions

All three keywords — `type`, `claim`, and `schema` — produce identical AST nodes
(`SchemaDecl`) and are interchangeable at the runtime level.  The distinction is
a reading contract described in `docs/design/what-we-learned.md`:

**`type`** — Use for things that define the structure of a single record value.
A type is a noun: something you instantiate and hold.  The constraints inside it
are simple local invariants on its own fields — always true for any valid instance,
no external dependencies.

```evident
type GameState
    location  ∈ String
    inventory ∈ Seq(Item)
    turn      ∈ Nat

type DateRange
    start ∈ Date
    end   ∈ Date
    start ≤ end        -- local invariant on DateRange's own fields
```

**`claim`** — Use for relations across multiple values, traits, properties, and
constraint modules.  A claim is a predicate: something that holds or doesn't hold
for a given set of values.  Claims are used both in test files (as assertions to
verify) and as constraint modules that can be mixed into other claims or types.
The test-file convention `sat_*` / `unsat_*` is just one application.

```evident
-- Trait / constraint module: a reusable property
claim assignment_fits_schedule
    a        ∈ Assignment
    schedule ∈ Set Assignment
    ∀ b ∈ schedule : a.room = b.room ⇒ a.slot ≠ b.slot

-- Test assertion
claim sat_north_exit_exists
    ("entrance", "north", "forest") ∈ exits_map
```

The practical line: if the constraints are purely local to the type's own fields
→ `type`.  If they involve external data, multiple objects, or complex logic that
varies by context → `claim`.

**`schema`** — Avoid.  It is a synonym for `type` with no additional meaning.
Prefer `type` when the thing is a noun (has a shape); prefer `claim` when it is a
predicate (defines a relation or property).  The word `schema` does not appear in
human-written Evident source files.

**`..TypeName` (passthrough / trait composition)** — Brings another type's or
claim's fields and constraints directly into the current scope without a dotted
prefix.  Think of it as trait composition.  The included declaration is still a
`type` or `claim`; `..` is the composition mechanism.

## Composing Types and Claims

### Using a type inside a claim: `variable ∈ TypeName`

Declares a variable of that type.  All of the type's fields become accessible
as `variable.field`, and the type's invariants are automatically enforced.
Use this when a claim needs to reason about a structured object.

```evident
claim assignment_fits_schedule
    a        ∈ Assignment      -- a is an Assignment; a.room, a.slot available
    schedule ∈ Set Assignment
    ∀ b ∈ schedule : a.room = b.room ⇒ a.slot ≠ b.slot
```

### Using a claim inside a type: baking a property in

When every instance of a type should satisfy a property, put the claim's
name directly in the type body.  The names-match rule identifies variables
automatically.

```evident
type ValidSchedule
    slots   ∈ Seq(TimeSlot)
    budget  ∈ Nat
    no_conflicts     -- claim; 'slots' matches by name
    within_budget    -- claim; 'budget' matches by name
```

This creates a **refined type** — a subset of all schedules that satisfy
those additional properties.  Use it when the constraint should always hold
for any valid instance, with no external data needed.

### Passthrough `..`: flat mixin, no prefix

`..SomeType` or `..SomeClaim` brings all fields into the current scope
without a dotted prefix.  The included constraints also apply.

```evident
type main
    ..LineReader    -- adds line, line_ready, src.* directly into scope
    ..LineWriter    -- adds line_out, dst.* directly into scope
    state ∈ GameState
```

Use passthrough when the fields of the included type/claim ARE fields of
the current type — not a sub-object you reference by name.  `..LineReader`
makes `line` available directly; `reader ∈ LineReader` would make it
`reader.line`.

### Names-match composition: zero-argument claims

When variable names in scope match a claim's variable names, just name the
claim — no explicit argument passing needed.  The solver identifies them.

```evident
claim valid_conference
    schedule     ∈ Set Assignment
    rooms        ∈ Set Room
    max_parallel ∈ Nat

    rooms_conflict_free    -- 'schedule' flows automatically by name
    parallel_load_within   -- 'schedule', 'max_parallel' flow by name
```

### Interface vars on the claim line + positional invocation

When a claim takes parameters, put them on the claim line so
callers can use positional invocation without `mapsto`:

```evident
claim Distinct(s ∈ Seq, n ∈ Nat)
    ∀ i ∈ {0..n-1} : ∀ j ∈ {0..n-1} : i < j ⇒ s[i] ≠ s[j]

claim my_problem
    items ∈ Seq(Int)
    #items = 8
    Distinct(items, 8)             -- positional, no `mapsto` needed
```

The first-line params are the claim's **interface** — what the
caller must supply. Body-level decls are internal helpers.

**Rule**: any var the caller supplies belongs on the claim line.
Internal helpers stay in the body.

### Generic Seq parameters: `s ∈ Seq` (no element type)

A claim parameter declared as `s ∈ Seq` (bare, no element type)
takes its element type from the caller's binding via names-match.
The same claim works for any element type whose body operations
are type-agnostic (distinctness, sortedness, …):

```evident
claim Distinct
    s ∈ Seq                  -- generic
    n ∈ Nat
    ∀ i ∈ {0..n-1} : ∀ j ∈ {0..n-1} : i < j ⇒ s[i] ≠ s[j]
```

`stdlib/distinct.ev` and `stdlib/sorted.ev` use this — single
generic claim, not per-type variants. Don't use when the body's
translation depends on the element type — give a concrete
`Seq(Bool)` so the type-check fires at the call site.

### Chained-membership with comparison chains

Beyond the basic `name ∈ Type = expr` form (covered above in
"Idiomatic Evident"), `∈` can sit inside a chained-comparison
expression — declare + bound in one line:

```evident
pos_x ∈ Int < 5            -- declare + upper bound
0 < pos_x ∈ Int < 5        -- declare + range (replaces 3 lines)
0 ≤ score ∈ Nat ≤ 100
val ∈ Int ≠ 0
x, y, z ∈ Int < 5          -- multi-name (3 decls, each bounded)
```

Each comparison pair desugars to its own `Constraint`. The
variable must be a bare identifier (no field access), and the
chain detector requires a line-end after the chain (so
`x ∈ pts ∧ x > 0` parses as Bool set-membership, not chained).

### Renaming with `↦`: when names differ

```evident
claim manage_event
    assignments ∈ Set Assignment
    Conference.valid (schedule ↦ assignments)  -- rename to match
```

### `subclaim`: nested claim scoped to a parent

A `subclaim` is a claim definition nested inside another claim's body.  It
has access to all of the parent claim's variables by name, but its
own internal variables are fresh and not visible to the parent.

```evident
claim GameTransition
    _state     ∈ GameState
    state      ∈ GameState
    response   ∈ String
    verb       ∈ Verb

    subclaim LookAction
        -- _state, state, response, verb are inherited from parent
        state.location = _state.location
        (_state.location, room_desc) ∈ room_descriptions
        response = room_desc

    subclaim GoAction
        -- direction, dest are internal to GoAction — not in parent scope
        direction ∈ String
        dest      ∈ String
        (_state.location, direction, dest) ∈ direction_exits
        ...
```

Use subclaims when a claim's dispatch logic is complex enough to name,
but the branches are implementation details not independently composable.

### `⟸` (reverse implication): dispatch tables

`A ⟸ B` means `B ⇒ A` (A applies when B).  It's syntactic sugar that
makes verb-dispatch tables read naturally:

```evident
-- "GoAction applies when verb = Go"
GoAction ⟸ verb = Go

-- Equivalent (but reads backward):
verb = Go ⇒ GoAction
```

Use `⟸` in dispatch tables where the consequent is named and the
condition is the selector.

### Decision guide

| Situation | Pattern |
|---|---|
| A claim needs one structured object | `variable ∈ TypeName` in the claim |
| A type should always satisfy a property | name the claim in the type body |
| Fields should live flat in scope (no prefix) | `..TypeName` or `..ClaimName` |
| Reusing a claim whose variable names match | name it directly (names-match) |
| Reusing a claim with different variable names | name it with `(x ↦ y)` |
| A subset of a type with extra invariants | define a new `type` that names the original type and adds constraints |
| Named dispatch branches inside a parent claim | `subclaim` + `⟸` |
| Multiple variables sharing a type | `x, y, z ∈ Int` (multi-name shorthand) |
| Declare and constrain in one line | `pos_x ∈ Int = 5`, `pos_x ∈ Int < 5`, or `0 < pos_x ∈ Int < 5` (chained-membership) |
| Compact short-record type definition | `type IVec2(x, y ∈ Int)` (first-line param list) |
| Construct a record value inline | `IVec2(380, 280)` positional, or `IVec2(x ↦ 1, y ↦ 2)` named |
| Componentwise comparison/equality of records | `a ≤ b`, `a = b`, `a ≠ b` lift automatically |
| Record-valued arithmetic equation | `c = a - b` lifts componentwise |
| Bounding-box / chained range on a record | `lo ≤ vec ≤ hi` (vector chained comparison) |
| Iterate parallel sequences | `∀ (a, b) ∈ coindexed(seqA, seqB) : …` |
| Iterate consecutive pairs of one sequence | `∀ (a, b) ∈ edges(seq) : …` |
| Inline a claim only when a condition holds | `cond ⇒ ClaimName` (guarded invocation) |
| Pin some fields of a record at declaration | `name ∈ Type (slot ↦ v)` or `name ∈ Type(v1, v2)` |
| Choose between two unrelated sources (use sparingly — see "Ternary is a fork" below) | `(cond ? a : b)` — ternary; both branches same sort, lowers to Z3 `ite`. For clamping prefer `lo ≤ x ≤ hi`; for dispatch prefer `subclaim` + `⟸`; for discrete-input → output, prefer a complete lookup table |
| Pattern-match an enum-typed scrutinee | `match e \n   Ctor(b) ⇒ body \n   _ ⇒ fallback` — indented arms, lowers to nested ITE |
| Test whether an enum value's variant is X (Bool result) | `e matches Ctor(_, _)` — recognizer; payload binds ignored. Use `match` to extract values, `e = Ctor(7)` for literal-payload comparison |
| Build a `Cons/Nil`-shaped enum value (EffectList, ResultList, ArgList, user LinkedList) | `var = ⟨a, b, c⟩` — lowers to `Cons(a, Cons(b, Cons(c, Nil)))`. Empty `⟨⟩` = `Nil`. Works inline in `match` arms when the LHS hints the enum type |
| Assemble a `Seq(T)` from named chunks | `xs ∈ Seq(T) = ⟨…⟩` then `out = a ++ b ++ ⟨c⟩` — `++` flattens at load time |

## Records as vectors

A short record type used as a value carrier (positions, colors, sizes,
velocities) gets first-class support throughout the runtime. Define
the type once with the multi-name shorthand:

```evident
type IVec2(x, y ∈ Int)
type Color(r, g, b ∈ Nat)
```

Once defined, four lifting forms work automatically:

**1. Componentwise comparison and equality**
```evident
pos_lo ≤ dot.pos ≤ pos_hi    -- pos_lo.x ≤ pos.x ≤ pos_hi.x ∧ same for y
a < b                         -- componentwise (every axis strict)
a = b                         -- componentwise
a ≠ b                         -- some-field-differs (disjunctive)
```

**2. Arithmetic broadcast in equation context**
```evident
c = a - b                     -- c.x = a.x - b.x ∧ c.y = a.y - b.y
nxt.pos = cur.pos + cur.vel * input.dt / 1000
state.dots[i] = src            -- whole-element record assignment via Index LHS
```

The lift sees `Identifier`, `Field-of-Index`, and `Index` records
(e.g. `dots[i]`), composes through `Binary` arithmetic, and
substitutes per-leaf. Shape mismatches (Vec2 = Vec3, etc.) are fatal
via the dropped-constraint policy — no silent partial-overlap.

**3. Type-use pins at declaration sites**
```evident
vel_lo ∈ IVec2 (x ↦ -800, y ↦ -800)   -- named, order-independent, partial allowed
pos_hi ∈ IVec2(740, 540)               -- positional, declaration order
sky    ∈ Color(30, 80, 120)
```

Both desugar to declaration + per-field equality. Named is partial
(omit fields to leave them free); positional requires args ≤ field
count and pins the leading fields.

**4. Record literals in expression position**
```evident
state.player.pos = IVec2(380, 280)
rect.pos   = dot.pos - IVec2(12, 12)
rect.color = Color(80, 200, 180)
```

Same `Type(args)` syntax as positional pins, used as a value-producing
expression. Lifts through equality and arithmetic identically to the
declaration form. Also valid as an inline argument to a claim call —
positional or `mapsto`:

```evident
set_draw_color(ren, Color(220, 40, 60, 255), out)   -- positional
use_color (c ↦ Color(7, 8, 9), sum ↦ s)             -- mapsto
```

The runtime expands the literal per-field and binds `slot.field` to
each arg before inlining the claim's body.

## N-arity sequence iteration

`coindexed(seqA, seqB, …)` zips parallel sequences by index;
`edges(seq)` iterates adjacent `(seq[i], seq[i+1])` pairs. Both use
tuple binding and require pinned lengths.

```evident
∀ (cur, nxt) ∈ coindexed(_state.dots, state.dots) :
    nxt.pos = cur.pos + cur.vel * input.dt / 1000

∀ (cur, nxt, eff) ∈ coindexed(_state.dots, state.dots, effective_vy) :
    -- per-dot physics referencing both snapshots and a parallel intermediate

∀ (a, b) ∈ edges(items) : a ≤ b   -- monotonicity
```

**Always prefer these over `∀ i ∈ {0..#seq - 1}` indexed loops.** The
tuple binding makes "what's being paired" visible at the call site;
the integer index never appears in the body.

**Caveat: parallel-Seq lengths must be pinned in `type main`'s body.**
The seq-length pinning preprocessor (`collect_seq_lengths`) only scans
the entry schema's body items. Seqs declared inside subclaims or
referenced through claim parameters won't have their `coindexed`
length pinning visible. Declare the Seqs in main, even if only an
inner subclaim uses them.

## Seq concatenation with `++`

Build a `Seq(T)` by naming subsequences and joining them with `++`.
A pre-translation pass (`desugar_seq_concat`) walks the body, gathers
`name = ⟨items⟩` bindings, then rewrites every `Concat` subtree
into a single flat `SeqLit` at load time. The translator never sees
`++` — it sees the already-flattened literal.

```evident
sky_clear   ∈ Seq(Effect) = ⟨sky_eff, clear_eff⟩
scene_draws ∈ Seq(Effect) = ⟨ground_color_eff, ground_fill_eff,
                              hat_color_eff,    hat_fill_eff,
                              shirt_color_eff,  shirt_fill_eff⟩
input_poll  ∈ Seq(Effect) = ⟨pump_eff, key_left_eff, key_right_eff⟩

effects = (halting ? ⟨Println("done"), Exit(0)⟩
           : sky_clear ++ scene_draws ++ ⟨present_eff⟩
               ++ input_poll ++ ⟨delay_eff⟩)
```

The rewrite recurses through `Ternary`, `Match` arms, claim-call
arguments, and further `Binary` operations — so `++` works wherever
a `Seq(T)` value is expected. The use case is reading-clarity: the
frame's effects read as "what it's composed of, by intent" instead
of one 18-element flat list.

**Operands must be statically resolvable.** Each leaf has to be
either a `SeqLit` literal or an `Identifier` that names a body-level
`name = ⟨...⟩` binding. Opaque `Seq` vars (e.g. coming from a claim
invocation that produces a Seq) won't flatten — that subtree is left
alone and the translator surfaces the usual "couldn't translate to
Bool" error pointing at it. Inline the chunks at the call site, or
push the assembly down into the producing claim.

## Guarded claim invocation

`condition ⇒ ClaimName` inlines the claim's body but wraps each
constraint in `condition ⇒ …`. Declarations from the claim fire
unconditionally; only constraints get guarded. Composes with
names-match — the claim's parameters resolve to outer-scope variables
of the same name without explicit `mapsto`.

```evident
claim InitGameState
    state ∈ GameState
    input ∈ SDLInput
    init_seeds ∈ Seq(Int)
    -- … initialization constraints …

fsm main(state ∈ GameState)
    input ∈ SDLInput
    init_seeds ∈ Seq(Int)
    -- … other setup …
    state.step = 0 ⇒ InitGameState   -- runs Init's constraints only on frame 0
```

Useful for one-shot setup ("first frame"), conditional behavioral
modes, or anywhere you'd otherwise inline a guard onto every
constraint of a named concern.

## Style: keep main compact

`type main` should read as **setup + configuration + claim wiring**,
not as a place where logic lives. Aim for ~80–100 lines for a
non-trivial game/simulation. Six tools that compound:

1. **Multi-name + first-line params for short types** —
   `type IVec2(x, y ∈ Int)` over four lines.
2. **Positional pins for short type instantiation** —
   `pos_lo ∈ IVec2(20, 20)` over two field equalities.
3. **`coindexed(...)` / `edges(...)` over indexed loops** — drop
   `∀ i ∈ {0..#seq - 1}` whenever the body operates on parallel-seq
   elements at the same index, or on adjacent pairs.
4. **Extract per-frame concerns into claims** — bounds, physics,
   render, collection, win, audio each become a one-line invocation
   from main; the claim body owns the `∀` and the per-element logic.
5. **Guarded claim invocation for one-shot logic** — `state.step = 0
   ⇒ InitGameState` reads as "run Init when initializing".
6. **`++` over a flat effects list** — name the chunks by intent
   (`sky_clear`, `scene_draws`, `input_poll`) and assemble with `++`.
   Reads as "what the frame is composed of" instead of an N-element
   list of named LibCalls.

(Earlier `sdl_demo/` engine + game pair is gone — the canonical
split is now embodied across `examples/test_NN_*.ev`. When we
build a richer game demo it should follow the same shape: an
engine claim file in `stdlib/` for reusable per-frame logic,
the game-specific types and aesthetic choices in the demo file.)

### Comments

Names carry the meaning. Section headers with one-line context are
fine; do not paragraph-explain every constraint. Counter-example to
avoid:

```evident
-- Update the dot's x position by adding velocity * dt to current.
nxt.pos.x = cur.pos.x + cur.vel.x * input.dt / 1000
```

The code already says this. Comment when the WHY isn't obvious — a
hidden invariant, a runtime caveat, an "I tried the obvious thing and
it broke" note. Otherwise let the names speak.

## Ternary is a fork, not a constraint

`(cond ? a : b)` lowers to a Z3 `ite`. The solver sees two disjoint
branches with no relation between them. A program built out of
stacked ternaries is **imperative branching dressed as constraints**
— the same shape an interpreter would walk, without the structural
insight that justifies using a solver in the first place.

The more ternaries that fork a single derived value, the more the
constraint model has been replaced by hand-written control flow.
Reach for it sparingly.

**When ternary is OK**
- One branch is a different *source*, not a different *value of
  the same thing*. `is_first_tick ? initial : computed` is a fork
  between "bootstrap" and "ongoing" — there's nothing relational
  to factor out.
- A single, exclusive, non-stacked split where the alternatives
  (subclaim, lookup table) would be more noise than signal.

**When ternary is a smell — reach past it**
- **Clamping to a boundary.** `(x < lo ? lo : (x > hi ? hi : x))`
  is `lo ≤ x ≤ hi` (chained comparison) — Evident lifts that
  componentwise for records: `bounds.lo ≤ pos ≤ bounds.hi`.
- **Discrete input → output.** A nested ternary over
  `key_left` / `key_right` is a dispatch table. Build a complete
  lookup: `(left, right, vx) ∈ walk_table`, with one row per
  input combination (including the no-input row — see "complete
  lookup pattern" under Program Structure).
- **Entity-state dispatch.** Branching on "on the ground vs in
  the air" reads better as `subclaim` + `⟸`:
  ```evident
  subclaim Grounded ⟸ pos.y = floor_y
      next_vel.y = (jump_pressed ? -jump_strength : 0)
  subclaim Airborne ⟸ pos.y < floor_y
      next_vel.y = _vel.y + gravity
  ```
- **Hardcoded numeric boundaries.** `pos.y > 400 ? 400 : pos.y`
  bakes the floor into the physics. Promote the boundary to a
  record (`AABB`, `WorldBounds`, `StaticBody`) and let the
  entity shapes drive the constraint. Adding a new platform
  then means adding an entity, not editing every ternary that
  hardcodes `400`.

**Signal**: ≥ 2 ternaries in a row referencing the same hardcoded
constant (window edge, floor `y`, sprite size) means you're
inlining an entity system. Define the entities and the relations,
and the ternaries dissolve.

## Parallel Seqs are forbidden

If you ever find yourself reaching for two Seqs that are *supposed
to line up* — `from ∈ Seq(Int)` and `to ∈ Seq(Int)` representing
edges, `xs ∈ Seq(Int)` and `ys ∈ Seq(Int)` representing points,
`names ∈ Seq(String)` and `ages ∈ Seq(Int)` representing people —
**stop**. Use a record type.

```evident
-- Don't:
from ∈ Seq(Int)
to   ∈ Seq(Int)
#from = #to    -- and now hope nothing else breaks the invariant

-- Do:
type Edge(from, to ∈ Int)
edges ∈ Seq(Edge)
```

**Why this matters more in Evident than in a normal language.** Z3
silently assigns values to anything you leave unconstrained. If
you have parallel Seqs and the length-equality drifts (or you
forget to write `#from = #to`), Z3 picks a "solution" by filling
in whatever fits — silently. You won't get a type error or a
runtime panic; you'll get *the wrong answer*, indistinguishable
from a real answer to a model consumer. The structural invariant
"these two Seqs are paired" can't be enforced by the type system,
only by hand-written constraints, and missing constraints in
Evident are silent bugs.

A record type makes the pairing *structural*. Two fields move
together by construction; there's no way to misalign them.

**Symptoms that mean you've drifted toward parallel Seqs**:
- `#a = #b` appearing as a constraint.
- A `∀ k ∈ {0..#a - 1}` whose body references `a[k]` *and* `b[k]`.
- "Did I remember to update both lists when I added an entry?"
  as a question you ever have to ask.
- A reviewer mentally zipping two Seqs to read a constraint.

Each of these is the type system asking to be a record.

**The mathematical generalization**: any relation between data is
a record. `Edge(from, to)` is a pair. `Map<K, V>` entries are
`Pair(key, value)`. `Coordinates(x, y, z)` is a triple. When you
hear yourself say "these are paired" or "indexed in lockstep" or
"the i-th of A matches the i-th of B" — that's a record begging
to exist.

## Indices in interfaces are a leak

If a claim's input or output traffics in `Int` indices to
identify "which item we mean", the interface is leaking an
implementation encoding into the contract. **Domain types in,
domain types out.** Indices are for internal computation; they
have no place at the API boundary.

```evident
-- Don't (output is index assignments):
claim Sort
    items ∈ Seq(Rect)
    position ∈ Seq(Int)        -- output: where each item lands
    -- caller has to invert: sorted[position[i]] = items[i]

-- Do (output is the sorted thing):
claim Sort
    items ∈ Seq(Rect)
    sorted ∈ Seq(Rect)         -- output is in the domain
```

**Why this matters.** Indices ARE a valid encoding of "which one"
— but they're an *implementation choice*, not a property of the
domain. A topological sort operates on graphs of nodes; nothing
in the math says nodes are integers. A sort operates on
comparable values; nothing says they're indexed. When the
interface returns indices, every caller has to do the same
"map → solve → unmap" boilerplate, AND every reader has to hold
that extra layer in their head. The cost is paid N times so the
implementation can save it once.

The implementation can still use indices freely. Just hide them.

**The rule**: if a parameter or output of a public claim has
type `Int` (or `Seq(Int)`) and its *meaning* is "an index into
some other variable", you're leaking. Either return the items
directly, or wrap the indices in a record type that carries them
along with the thing they index.

**When indices ARE legitimate at the interface**:
- They're a domain concept in their own right (a "tick number",
  a "frame index", an "event sequence number").
- The caller doesn't need to invert them; they're consumed as IDs.

If you write a claim and find yourself documenting "to use this,
the caller does the following lookup loop", the lookup loop
belongs *inside* the claim. Bring the indices in; surface the
domain type out.

See `docs/design/toposort.md` for the worked example — toposort
as a constraint problem, why the natural representation isn't
`Seq(Int) of positions` even though the implementation uses one.

## Iterate over elements, not over `{0..#seq - 1}` ranges

When you reach for `∀ i ∈ {0..#seq - 1} : ... seq[i] ...`, **stop**.
The range-of-integers form is a low-level fallback. The
language already lets you iterate elements directly, and for
record-element Seqs it auto-binds `.field` access on the
element name. Use that.

```evident
-- Don't (index-style):
∀ i ∈ {0..#edges - 1} :
    position_of(sorted, edges[i].from) < position_of(sorted, edges[i].to)
∀ i ∈ {0..#items - 1} :
    contains(sorted, items[i])

-- Do (element-style):
∀ e ∈ edges :
    position_of(sorted, e.from) < position_of(sorted, e.to)
∀ x ∈ items :
    contains(sorted, x)
```

**Why this matters.** Indices in the quantifier are an artifact
of "I'm walking a sequence by position." The math says "for
every edge in the graph, this relation holds" — the bound name
is *an edge*, not *the index of an edge*. The element form
matches the math; the index form makes you mentally unwind
"what's at position i" every time you read it.

**The element form is supported for both primitive and
record-element Seqs.** For a `Seq(Int)`, `∀ x ∈ s : x > 0`
binds `x` to each Int element. For a `Seq(Edge)`, `∀ e ∈
edges : e.from = ...` binds `e` as the element AND makes
`e.field` accessible for each field on the element record.
The runtime's `Forall` translator in
`runtime/src/encode/exprs/bool.rs` does the field-binding via
`bind_composite_fields` (in `exprs/equations.rs`) for
composite-element Seqs; primitive Seqs bind the element value to
the variable directly.

**When indices ARE necessary**:
- You need the position itself in the constraint (e.g. "the
  i-th element relates to the i-th element of another Seq" —
  but for that, `coindexed(A, B)` is usually cleaner; see
  "N-arity sequence iteration").
- You need to compare positions of two specific elements (e.g.
  `position_of(seq, x) < position_of(seq, y)` — but
  `position_of` is the right tool, not a `∀ i ∈ {0..n-1}` loop).
- You're computing a function of the index itself (rare).

For everything else: `∀ x ∈ seq` reads as the math and runs
the same constraints.

**The deeper point**: the range-over-integers form is
unrolled-by-the-runtime over a pinned length — Rust loops
through 0..n at translate time, generates a constraint per
iteration. The element form does *exactly the same thing*
under the hood, just with the element value bound to the name
instead of the integer. The work happens in the runtime
either way; the source-level form should be the one closer to
the math.

## Program Structure

Full guidance: `docs/design/program-structure.md`. Summary below.

### The layered stack

```
data layer     — enums and complete lookup tables (ground facts, no logic)
type layer     — pure structs and state snapshots (local invariants only)
trait layer    — small reusable behavioral claims
claim layer    — relations, dispatch, transition systems
entry point    — wiring only (passthroughs + variable declarations)
```

Each layer depends only on layers below it. The entry point (`type main`) should
contain no logic — only passthrough composition and variable declarations.

### Boolean literals are lowercase

`true` and `false` (lowercase). `True` and `False` (capitalized) parse as
unbound identifiers — the constraint is silently dropped and the variable
is left free. This produces no error, just wrong behavior.

```evident
state.done = true    -- correct
state.done = True    -- SILENT BUG: True is an unbound name, constraint dropped
```

### Precedence: `⇒` binds tighter than `∧`

**This is a footgun.** Evident's grammar has `⇒` at higher precedence than `∧` —
opposite of standard mathematical convention. So:

```evident
A ⇒ B ∧ C        -- parses as (A ⇒ B) ∧ C  ← wrong!
A ⇒ (B ∧ C)      -- correct: parentheses required for compound consequent
```

In a dispatch table, every consequent with multiple terms needs parens:
```evident
parsed.verb = Look ⇒ (StateTurn ∧ LookAction)   -- correct
parsed.verb = Look ⇒ StateTurn ∧ LookAction      -- WRONG: LookAction fires unconditionally
```

Alternatively, use an implies_block (indented body) to avoid the issue:
```evident
parsed.verb = Look ⇒
    StateTurn
    LookAction
```

### Precedence: `=` binds tighter than `∧` / `∨`

**Same family of footgun.** A boolean assignment that mixes `=` with logical
operators on the RHS needs outer parens or it splits into the wrong shape:

```evident
in_box = abs(x - cx) ≤ w ∧ abs(y - cy) ≤ h     -- WRONG
-- parses as ((in_box = abs(x-cx)) ≤ w) ∧ (abs(y-cy) ≤ h)
-- — a free-floating boolean expression, in_box is never assigned

in_box = ((abs(x - cx) ≤ w) ∧ (abs(y - cy) ≤ h))   -- correct
-- the outer parens scope `∧` inside the equation's RHS
```

Comparison operators (`<`, `>`, `≤`, `≥`) are also looser than `=`:

```evident
in_circle = length(p - c) < r       -- WRONG, parses as ((in_circle = length(...)) < r)
in_circle = (length(p - c) < r)     -- correct
```

Rule of thumb in shader bodies (or anywhere you assign a boolean result):
**always wrap the RHS in `( )` if it contains `<`, `>`, `≤`, `≥`, `∧`, `∨`, or
multiple `=` signs**. Costs nothing and the parser will tell you if you wrote it
wrong.

### The complete lookup pattern

Partial lookup tables cause Z3 non-determinism. If a table only has entries for
valid cases, Z3 can satisfy `(A, B, result) ∈ table ⇒ body` by choosing a
non-matching `(A, B)` to make the antecedent false.

Fix: make every table complete, using a sentinel (e.g. `""`) for "nothing":
```evident
assert direction_exits = {
    ("entrance", "north", "forest"),
    ("entrance", "south", ""),     -- blocked: sentinel, not absent
    ...
}
```
Then branch positively on the result: `dest ≠ "" ⇒ ...` / `dest = "" ⇒ ...`.

### Variable scope planning

Parent-level variables = the public interface (everything subclaims share).
Subclaim-internal variables = implementation details used by one branch only.

If a variable appears in only one subclaim, declare it inside that subclaim
(it becomes a fresh Z3 constant, not visible to the parent or other subclaims).

### Constraint scope rule

**Constraints referencing external data cannot live in a type body.**

When `item ∈ Item` is expanded, the sub-env contains only Item's own fields.
A constraint like `(kind, name) ∈ item_names` would be silently dropped because
`item_names` is not in that sub-env. Move it to the claim where the global fact
is in scope.

### Naming conventions

- **Enums**: `ItemKind`, `Verb` — name the set of identity values
- **Pure structs**: `Item`, `ParsedCommand` — noun, no external constraints
- **Traits**: `PreservesInventory`, `AdvancesTurn` — adjective/present-participle
- **Action subclaims**: `LookAction`, `GoAction` — noun phrase naming the branch
- **Dispatch**: `ActionName ⟸ condition` reads "ActionName applies when condition"

### Diagnostic questions

- Are all lookup tables complete? Any partial table risks Z3 non-determinism.
- Do any type bodies reference lookup tables? Move those constraints to the claim.
- Are there variables that always appear together? They may be a type.
- Are there repeated constraint patterns across branches? They may be a trait.
- Can you name each dispatch branch? If not, it may need further decomposition.
- Does the parent declare variables only one subclaim uses? Move them inside.

## Key Invariants

**Enums**
- Top-level `enum Color = Red | Green | Blue` with the dedicated
  `enum` keyword (not `type`). Payload variants, self-recursion,
  forward references, and cross-enum mutual recursion all work
  (`enum Result = Ok(Int) | Err(String)`,
   `enum LL = Nil | Cons(Int, LL)`,
   `enum A = X(B) ; enum B = Y(A)`). Variant names are GLOBALLY
  unique across all enums; duplicates fail at load.
- Multiple enum decls per file batch through Z3's
  `create_datatypes` so forward / mutual references resolve in
  one pass.

**Variable scoping**
- Variables declared inside a schema body are local to that
  schema's query.
- A sub-schema membership `task ∈ Task` expands into per-field
  Z3 leaves (`task.id`, `task.duration`, …). The bare `task`
  variable is never stored in env; only the leaves are.
- Type names can shadow as variable names without conflict —
  they live in different namespaces.

**Subclaims**
- `subclaim Name` inside any schema body registers a top-level
  schema at load time. Available for names-match composition,
  receiver-prefix dispatch, or subschema-of-type dispatch.
- Subclaim-internal vars are fresh per-invocation; not visible
  to the parent.
