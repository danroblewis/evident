# Port the Evident Runtime

You are implementing an **Evident** runtime in a host language of your
choice (C, Go, Python, OCaml, Common Lisp — anything that can call into
Z3 and `dlopen`/`libffi`).

Evident is a constraint programming language: programs are collections
of constraints over sets, and a Z3 SMT solver finds satisfying
assignments. The contract you are implementing is **`effect-run`** —
take a `.ev` file, load its FSMs, dispatch their effects until they
halt.

## What's in this directory

- **`PROMPT.md`** — the standing brief to hand to an agent doing the
  port. Has the task statement, staging order, working style, and
  the definition of "done". Start there if you're the implementer.
- **`SPEC.md`** — language-agnostic specification of the runtime.
  This is the source of truth. Start here. It covers:
  - external dependencies (Z3 + libffi)
  - the pipeline (lex → parse → translate → solve → dispatch → schedule)
  - which components must exist in the host vs can be in Evident
  - the bootstrap subset (minimum language features to load `stdlib/`)
  - a staged implementation order
- **`stdlib/`** — the standard library you must be able to load. The
  load-bearing files are `runtime.ev` (defines `Effect`, `Result`,
  `FFIArg` enums plus `EffectPair`) and a small set of inference /
  desugar helpers under `passes/` that the reference runtime
  auto-loads from cwd. Your runtime must accept `import "stdlib/…"`
  and resolve it relative to the working directory.
- **`examples/`** — four `.ev` programs of increasing complexity. Your
  runtime must run all four end-to-end via `effect-run`.
- **`expected/`** — golden outputs. For each example `NN_name.ev` there
  is `NN_name.txt` (expected stdout, exact bytes) and `NN_name.exitcode`
  (expected process exit code).
- **`conformance/`** — Python/pytest black-box test suite covering the
  language surface (`query`, `check`, parse errors, imports, language
  features). Run with `EVIDENT_CMD=./your-evident pytest
  runtime-port/conformance/`. 119 tests; all pass against the Rust
  reference.

## The examples, in order of complexity

1. **`01_hello.ev`** — smallest possible multi-FSM program. One FSM,
   two states (`Init`, `Done`). Init emits `Println` + `Exit(0)`.
   Verifies: lex/parse, enum, fsm-shape detection, `match`, `Seq(Effect)`
   literal `⟨…⟩`, dispatch of `Println` and graceful exit via `Exit`.
2. **`02_counter.ev`** — multi-step FSM with `last_results` feedback
   and the `IntToStr` effect that produces a `StringResult`. Verifies:
   multi-tick scheduling, result→effect plumbing, integer arithmetic
   on tick state.
3. **`03_exit_code.ev`** — `Exit(42)` must propagate to the process
   exit code, not just exit 0. Verifies: `LoopResult::exit_code` flow.
4. **`04_two_fsms.ev`** — two FSMs sharing a `World` record (producer
   counts down, consumer reads and prints). Verifies: subscription-
   driven scheduling, world reads/writes, `_world.X` previous-tick
   semantics, deterministic interleaving.

None of these examples require FFI. FFI is the next milestone after
these four pass; once those work, the spec's section on `LibCall`
covers the marshalling and the dynamic loader.

## How to verify your runtime

### Examples (the executor side, `effect-run`)

For each example:

```
your-evident effect-run runtime-port/examples/NN_name.ev > actual.txt
echo $? > actual.exitcode
diff runtime-port/expected/NN_name.txt actual.txt
diff runtime-port/expected/NN_name.exitcode actual.exitcode
```

Both diffs must be empty. Exact-byte match on stdout, exact match on
exit code. No trailing-newline games.

### Conformance suite (the language side, `query` / `check`)

Once your runtime can parse, translate, and dispatch to Z3 — even
before `effect-run` works — the conformance suite is the broader
correctness target:

```
EVIDENT_CMD=./your-evident pytest runtime-port/conformance/ -q
```

119 tests covering enum membership, arithmetic, sequences, records,
claim composition, dispatch, syntax sugar (chained-membership,
`coindexed`, `++`), and CLI shape (`query --json`, `--given`, exit
codes, import resolution). The suite is host-agnostic; the only
configuration is `EVIDENT_CMD`. All 119 should pass against a
complete port.

## Where the reference implementation lives

The Rust runtime that produced these golden outputs lives in the parent
repo (`../runtime/src/`). Treat it as a reference, not a translation
target — your job is to implement against `SPEC.md`, not to port
Rust line-for-line. The reference is useful when the spec is
ambiguous: read the corresponding module and the spec at the same
time, then ask which behavior is load-bearing.

Key reference modules (paths relative to the parent repo):
- `runtime/src/lexer.rs` — Unicode operators + word keywords
- `runtime/src/parser.rs` — recursive-descent parser
- `runtime/src/ast.rs` — AST node types
- `runtime/src/translate/` — AST → Z3 sorts + constraints
- `runtime/src/effect_loop.rs` — multi-FSM scheduler
- `runtime/src/effect_dispatch.rs` — effect → IO

## Suggested staging

1. Get `01_hello.ev` running. This forces you through the entire
   pipeline at minimum capability: lex, parse, translate one fsm and
   one match, solve, dispatch `Println` and `Exit`.
2. Add the `last_results` plumbing and the `IntToStr` effect — that
   unlocks `02_counter.ev` and `03_exit_code.ev` together.
3. Add the subscription-driven scheduler and `_world` previous-tick
   semantics — that unlocks `04_two_fsms.ev`.

After all four pass, you have a working `effect-run` for the
non-FFI core. The FFI primitive (`LibCall` over libffi + dlopen)
is the next layer; the spec covers it but the examples in this
directory don't exercise it.

In parallel, run the conformance suite (`pytest
runtime-port/conformance/`). Many of its tests only need `query` /
`check` — those are reachable before the full scheduler is wired —
so the suite can be brought up incrementally alongside the
examples.

## Ground rules

- **Z3 is required.** Do not try to write your own SMT solver. Link
  against libz3 (any binding for your host language is fine).
- **The bootstrap subset matters.** You only need to support enough
  of the language to load `stdlib/runtime.ev` and the four examples.
  See SPEC.md's "Bootstrap subset" section.
- **Exact stdout.** The golden files are exact bytes. If your
  implementation prints debug logging by default, gate it behind a
  flag.

When in doubt, read SPEC.md again, then the reference module, then
ask.
