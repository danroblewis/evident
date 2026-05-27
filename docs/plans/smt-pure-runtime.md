# Plan: Make `runtime-smt` a PURE SMT-LIB runtime (no Evident)

## The drift this corrects

`runtime-smt` was supposed to be strategy 1: a **from-scratch runtime whose
input is SMT-LIB**. Instead it accreted a 4,631-LOC Evident transpiler
(`frontend.rs` + `fsm_frontend.rs`) — now *larger* than the 3,357-LOC engine —
and its headline subcommand (`fsm`) takes `.ev`, not `.smt2`. The Evident part
was supposed to come **much, much later**. It became the bulk of the crate.

Worse, it's redundant: the **main `runtime/` already owns the Evident→SMT-LIB
compiler** (`runtime/src/translate/smtlib.rs`, `evident dump-smtlib`). The
compiler belongs on the Evident-aware side. `runtime-smt` should only *consume*
SMT-LIB.

## Mission

`runtime-smt` consumes SMT-LIB and runs it. Its input is `.smt2` + `@meta`
(the metadata block declaring FSM state/effects). It contains **no Evident
parser, no Evident transpiler**. The primary interface is `run <file.smt2>`.

```
.smt2 + @meta  --->  runtime-smt (engine: scheduler / model / effect / z3 / cache)  --->  output
```

Where the `.smt2` comes from is NOT this runtime's concern: hand-written for
now; the main runtime's `dump-smtlib` later; a full Evident compiler much later.

## Non-negotiables

- Fully autonomous (`--dangerously-skip-permissions`). Never ask for approval.
- Additive/structural in `runtime-smt/` ONLY. Do NOT touch `runtime/`. Do NOT merge to main.
- No `#[ignore]`, no faked passes. Honest boundaries documented.
- Commit a checkpoint early; push your branch; work continuously.

## Orchestration protocol

Fan out parallel subagents per phase, integrate, gate, checkpoint. Each phase
ends green + pushed.

## Phases

### P1 — Cordon the Evident frontend OUT of the runtime
Move `frontend.rs` + `fsm_frontend.rs` and the `fsm` / `transpile` subcommands
out of the runtime's core into a clearly-separated, **deferred** location — a
feature-gated `transpiler` module behind `--features evident-bridge`, OR a
separate `src/bin/evident2smt.rs`. Default build of `runtime-smt` must NOT
compile the Evident frontend. The `runtime-smt` library + its default binary
expose only `solve <file.smt2>` and `run <file.smt2>`. Keep the transpiler code
(it's future work done early) but it is NOT part of the runtime.

### P2 — `run <file.smt2>` is the runtime
Make `run` execute the full `@meta` FSM format (state threading, multi-FSM
shared world, effects, caching) directly from a `.smt2` file — no `.ev` anywhere
in the path. This is the engine's single entry point.

### P3 — Grow a HAND-WRITTEN `.smt2` + `@meta` fixture suite
The runtime's coverage is measured by **SMT-LIB capability**, NOT by Evident
examples. Author `.smt2` + `@meta` fixtures (extending the existing 5 in
`runtime-smt/fixtures/`) that exercise, each with an expected-output golden:
scalar Int/Bool state, enum/datatype state, payload datatypes, multi-FSM shared
world, effect sequences (Println/Exit), the transition cache, halt semantics.
A `fixtures/MANIFEST.md` lists each fixture + the capability it pins. Tests run
every fixture and assert its golden.

### P4 — Docs: the runtime's true identity + honest LOC
Rewrite `runtime-smt/README.md`: input is `.smt2` + `@meta`; the engine is the
whole runtime; the Evident bridge is a separate, deferred, feature-gated tool.
State the LOC split: engine (runtime) vs cordoned transpiler (not built by
default). Update `COVERAGE.md` to measure SMT-LIB-runtime capability via the
fixture suite, not transpiled `.ev` count.

## Gates (all green before done)
- `cd runtime-smt && cargo build --release` (default features) compiles **without** the Evident frontend.
- `cd runtime-smt && cargo test --release` green; every hand-written fixture passes its golden.
- `runtime-smt run <fixture>.smt2` works for every fixture; NO `.ev` on the default path.
- Branch pushed, NOT merged.

## Note
The convergence-with-the-real-runtime tests that depend on the Evident
transpiler may move behind `--features evident-bridge` (they belong to the
bridge, not the runtime). That is correct — parity-via-transpile is a property
of the compiler, not the SMT-LIB executor.
