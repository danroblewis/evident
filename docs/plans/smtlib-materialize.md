# Plan: Materialize the runtime logic as on-disk `.smt2` files

## Mission

Today `runtime-smt fsm <file.ev>` generates SMT-LIB **in memory** (a transient
`String` from `transpile_fsm`) and feeds it to Z3. The program logic therefore
lives in **Rust**, not in SMT-LIB files. The stated goal is the opposite: the
**majority of the program logic should live in `.smt2` files**, with a thin
native engine that loads and runs them.

This session makes that real. Split the pipeline at the SMT-LIB boundary:

```
Evident  --(compile, AOT)-->  <name>.smt2 file on disk  --(run)-->  native engine + Z3
```

The `.smt2` file becomes the **source of truth for the program's behavior**.
The native engine (scheduler / model-extract / effect-dispatch / Z3 binding) is
the *only* Rust that runs at execution time — and it is irreducibly native
(SMT-LIB has no loops or IO; the z3-audit established this). So the achievable
target is precisely: **program logic = `.smt2` files; engine = thin native shell.**

## Non-negotiables

- Fully autonomous (`--dangerously-skip-permissions`). Never ask for approval.
- All work additive in `runtime-smt/`. Do NOT touch `runtime/`. Do NOT merge to main.
- Commit a checkpoint as soon as you have a candidate; push your branch; work continuously.
- No `#[ignore]`, no faked passes. Honest boundaries documented.

## Orchestration protocol

Fan out parallel subagents per phase, integrate, gate, checkpoint. Each phase
ends with: runtime-smt suite green + the phase's convergence check byte-identical
+ a pushed commit.

## Phases

### P1 — `compile` subcommand (Evident → `.smt2` file)
Add `runtime-smt compile <file.ev> -o <out.smt2>`. It runs `transpile_fsm` and
**writes the full self-contained SMT-LIB to disk**: the `@meta` block, datatype
decls, and every per-transition `@transition` section — everything `run` needs
to execute it with no further reference to the `.ev` source.

### P2 — one code path: `fsm` = `compile` then `run`
`run <file.smt2>` already executes fixtures. Make it execute the **full `@meta`
FSM format** that `compile` emits, and refactor `fsm <file.ev>` to internally
`compile` to a temp `.smt2` then `run` it — so there is exactly ONE execution
path and the in-memory vs on-disk forms cannot drift. Prove
`fsm X.ev` ≡ (`compile X.ev -o t.smt2`; `run t.smt2`) byte-identical.

### P3 — check in the compiled corpus + convergence tests
For each of the 12 byte-identical examples (see `runtime-smt/COVERAGE.md`),
compile to `runtime-smt/compiled/<name>.smt2` and **commit the `.smt2` files**.
Add a convergence test per file: `run compiled/<name>.smt2` == `evident
effect-run examples/<name>.ev` (exit code + stdout). These checked-in `.smt2`
files ARE the demonstration that the logic lives in SMT-LIB.

### P4 — docs: the architecture story
Update `runtime-smt/README.md` + `COVERAGE.md`: the program logic lives in the
`compiled/*.smt2` files; the engine loads them; the only execution-time Rust is
the (irreducibly native) scheduler/dispatch/Z3-binding. State the honest LOC
split: logic-in-SMT-LIB vs engine-in-Rust.

## Gates (all green before declaring done)
- `cd runtime-smt && cargo test --release` green.
- `compile` then `run` byte-identical to `fsm` AND to `evident effect-run` on all 12.
- `runtime-smt/compiled/*.smt2` committed; convergence tests pin each.
- Branch pushed, NOT merged.

## Honest-note requirement
If any example's logic cannot be fully captured in a standalone `.smt2` (e.g. it
needs host state the SMT-LIB can't carry), document it in COVERAGE.md as a
boundary — do not fake it.
