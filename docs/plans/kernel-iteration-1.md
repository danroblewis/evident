# Iteration 1: kernel + SMT-LIB emit

End state: a **tiny kernel** (`kernel/`) that runs Evident programs
by trampolining Z3 + libffi, plus an **`evident emit`** subcommand
in the existing Rust runtime that produces SMT-LIB the kernel
consumes.

After this iteration:
- `evident sample <file> <claim>` — unchanged (Z3 model, no I/O)
- `evident emit <file> <claim>` → `out.smt2` (new)
- `kernel out.smt2` → actually runs (new)

The Rust runtime still owns lex/parse/translate. The kernel is just
the trampoline. No self-hosting yet; that's iteration 2+. This
iteration is the wedge that unblocks it.

(Revised from v1 per the plan critique:
`docs/sessions/*kernel-iteration-1-critique*`. Major changes:
`Exit` is back as an `Effect` variant, not a field; D1 sized up
from 300-500 → 800-1200; D2 sized up 200 → 300; build order flips
to spec-first; single-SeqLit-producer rule added; deferred
Effect-toposort acknowledged.)

## What the kernel does

```
state         = initial_state
last_results  = empty
loop:
   model       = z3.solve(smtlib, state, last_results)
   state_next  = model.read_field("state.*")        -- flat fields per manifest
   effects     = model.read_field("effects")        -- Seq(Effect)
   new_results = []
   for eff in effects:                              -- walk in SeqLit order
      r = perform(eff)                              -- println / libcall / read / write / …
      new_results.append(r)
      if eff matches Exit(code):
         exit code                                  -- short-circuit; rest of Seq dropped
   if state_next == state: exit 1                   -- stuck halt (no Exit emitted, no progress)
   state        = state_next
   last_results = new_results
```

Halt conditions, in priority order:
1. **Explicit** — `effects` contains an `Exit(code)` variant. The kernel
   performs effects in Seq order; the moment it dispatches `Exit(code)`,
   it exits with that code. Effects appearing AFTER `Exit` in the Seq
   are dropped.
2. **Stuck** — `state_next == state` and no `Exit` was emitted. Exit 1.
   FSMs are supposed to decide their own halt; reaching a fixpoint
   without saying so is a bug.
3. **UNSAT on a tick** — Z3 says the tick's constraints are unsatisfiable
   (typically: a user invariant + observed `last_results` contradict).
   Exit 2.
4. **Internal error** — Z3 crash, libffi crash, OOM. Exit 3.

`Exit` is a regular `Effect` variant carrying an `Int`, no different in
shape from `Println(String)`. The kernel pattern-matches on its
variant name.

## What the FSM exposes to the kernel

Two top-level concepts the kernel reads from the model:

| Field | Type | Meaning |
|---|---|---|
| `state.*` (flat fields) | Per the FSM's state declaration | The tick's resulting state, with `_state` wired to last tick's value |
| `effects` | `Seq(Effect)` | The batch of effects to perform this tick |

No `exit` field. No multi-FSM scheduling concerns. The user's FSM is
responsible for producing a fully-pinned `effects` SeqLit (emit
enforces this — see "Single-writer rule" below).

### State encoding: flat fields with a manifest

The Rust runtime already represents composite state as flat
`state.field` keys in `env` (see `bind_composite_fields` in
`translate/exprs/seq_eq.rs:370`). The kernel uses the same shape.

**Manifest** — the SMT-LIB output declares a list of `state.*`
identifiers at the top, so the kernel knows:
1. Which top-level SMT-LIB constants to read from the model post-solve.
2. Which `_state.*` constants to pre-assert at the next tick's start.

Format (inline in the `.smt2` as `;; manifest:` comments):
```
;; state-fields: state.x:Int state.y:Int state.mode:AppState
;; effects-name: effects
;; effect-enum-name: Effect
```

The kernel reads the manifest header before invoking Z3.

### Single-writer rule for `effects`

`effects = ⟨a, b⟩ ∧ effects = ⟨c, d⟩` is UNSAT (different SeqLits).
Multi-writer composition uses `++`:

```evident
fsm logger
    effects_log = ⟨Println("tick")⟩

fsm worker
    effects_work = ⟨…⟩
    state.mode = (work_done ? "Done" : "Running")

fsm shutdown
    state ∈ AppState
    effects_exit = (state.mode = "Done" ? ⟨Exit(0)⟩ : ⟨⟩)

fsm main
    ..logger
    ..worker
    ..shutdown
    effects = effects_log ++ effects_work ++ effects_exit
```

**`evident emit` enforces**: the schema bound to `effects` must be
constrained by *exactly one* SeqLit-shaped constraint (which may
itself be a chain of `++`s — `desugar_seq_concat` flattens it at
load). Multiple equality constraints on `effects` → emit-time error.

Sub-FSM "effect contributions" use distinct Seq names; the assembly
FSM is the single writer for `effects`.

## The Effect enum

Lives in `stdlib/kernel.ev`. No `Exit` *field* anywhere — `Exit` is a
variant of `Effect`.

```evident
enum Effect =
    Println(String)
    Print(String)
    ReadLine                    -- result: StringResult(line) or EofResult
    ReadFile(String)            -- path → StringResult / ErrorResult
    WriteFile(String, String)   -- path, contents → NoResult / ErrorResult
    LibCall(String, String, Seq(LibArg))
                                -- lib, fn, args → IntResult / RealResult / StringResult
    Time                        -- → IntResult(unix-ms)
    Exit(Int)                   -- halt with code; short-circuits remaining effects

enum LibArg =
    ArgInt(Int)
    ArgStr(String)
    ArgReal(Real)

enum Result =
    NoResult
    IntResult(Int)
    StringResult(String)
    RealResult(Real)
    EofResult
    ErrorResult(String)
```

Everything I/O-shaped reduces to `LibCall` eventually; the named
variants are sugar wrapping common libcalls so simple programs
don't have to spell out `LibCall("libc.so", …)`.

## Iteration 1 deliverables

### D0. Kernel input spec (`docs/plans/kernel-input-spec.md`)

**Pre-requisite for D1+D2.** A short doc (one page) defining:
- SMT-LIB header conventions: required `(set-logic …)`, `(declare-datatypes …)` blocks for `Effect` and `Result`.
- Manifest comment format (`;; state-fields:`, `;; effects-name:`, `;; effect-enum-name:`).
- The Z3 model-read protocol: which top-level decls the kernel queries, in what order, expected sorts.
- Initial state convention: how the kernel pre-asserts tick-0 state (or whether the SMT-LIB itself includes a `(assert (= state.foo <init>))` block).
- `_state` wiring across ticks: kernel sets `(assert (= _state.x <prev-value>))` before each solve.
- `last_results` wiring across ticks: kernel sets the previous tick's `Seq(Result)` as a literal.
- Failure semantics: what the kernel does on UNSAT, libffi failure, malformed Effect variant, missing manifest, etc.

Without this doc, D1 (kernel) and D2 (emit) are coupled by undocumented
assumptions. With it, they can be built in parallel.

### D1. Kernel crate (`kernel/`)

A new Rust binary, **~800-1200 LOC** (revised from 300-500 per critique).
Dependencies: `z3-sys` and `libffi`.

What it does:
- `kernel <file.smt2>` — load, run, exit with the kernel's exit code.
- Read manifest header.
- Z3 solve loop as above.
- Effect dispatch: enum variant → built-in (Println/Print/ReadLine/Time/etc.)
  or libffi call (for `LibCall(_, _, _)`).
- Result marshalling back into the next tick's `last_results` literal.
- Effect pattern-match for `Exit(code)` → short-circuit.
- Error policy: per the spec doc — fold libcall errors into `ErrorResult(_)`
  (visible to the FSM), reserve exit codes 1-3 for halt-reason signaling.

Explicitly **NOT** in the kernel:
- Lexing, parsing, translation, type inference, generics — none of it.
- No knowledge of Evident syntax. Only SMT-LIB.
- No optimizer/MaxSMT. Pure SAT via Z3 Solver.

Approximate breakdown (per critique):
- Z3 SMT-LIB load + solver setup: ~80 LOC
- Manifest parsing: ~60 LOC
- Model walk for state fields: ~120 LOC
- `Effect` variant decoding (7+ variants): ~150 LOC
- `Result` variant encoding (back into SMT-LIB): ~100 LOC
- Libffi marshalling per arg type with cleanup: ~200 LOC
- Built-in dispatch table (Println/Time/ReadFile/…): ~150 LOC
- Error handling + diagnostics: ~80 LOC
- CLI plumbing: ~60 LOC

### D2. `evident emit` subcommand

**~300 LOC** (revised from 200 per critique — manifest emission adds ~100).
`runtime/src/emit.rs` + a CLI subcommand.

What it does:
- `evident emit <file.ev> <claim> [-o <out.smt2>]` — writes SMT-LIB to
  stdout or a file.
- Reuses the existing `translate/` pipeline up through `declare` and
  `inline`.
- Emits Effect / Result Datatype declarations.
- Emits state-field declarations + the manifest header.
- Emits `effects` as a Seq variable with the body constraints inlined.
- **Single-SeqLit-producer check**: scans the resolved schema body for
  constraints binding `effects`. If more than one SeqLit-equality
  constraint exists (after `++` flattening), emits a clear error and
  exits.

### D3. Effect / Result stdlib (`stdlib/kernel.ev`)

The enums above plus a handful of sugar claims for common operations:

```evident
claim Println(s ∈ String, eff ∈ Effect)
    eff = Println(s)

claim ReadFile(path ∈ String, eff ∈ Effect)
    eff = ReadFile(path)
```

Maybe 50-100 LOC total. The user is free to write `eff = Exit(0)` inline;
the claims are reading-convenience, not required.

### D4. End-to-end smoke test (`tests/kernel/`)

Smallest possible cycle:

```evident
-- tests/kernel/test_hello.ev
import "stdlib/kernel.ev"

fsm hello
    state.mode ∈ String = "Done"
    effects = ⟨Println("hello world"), Exit(0)⟩
```

Pipeline:
```
evident emit tests/kernel/test_hello.ev hello > /tmp/hello.smt2
kernel /tmp/hello.smt2     # should print "hello world", exit 0
```

A Python driver (`scripts/run-kernel-tests.py`, mirroring
`scripts/run-lang-tests.py`) drives every `tests/kernel/test_*.ev`
through the pipeline and asserts stdout match + exit code match.

Fixtures (about 5 starter programs):
- `test_hello.ev` — println + Exit(0)
- `test_echo.ev` — read-line, println-echo, loop until EofResult, Exit(0)
- `test_counter.ev` — counter using `_state` and a ternary, Exit when N reached
- `test_libcall.ev` — call libc's `getpid` via LibCall
- `test_unsat.ev` — programs that should exit 2

### D5. test.sh phase 5

Add a kernel phase to `test.sh`:
- Build `kernel/` crate (if not already built)
- Run `python3 scripts/run-kernel-tests.py`

## Open questions resolved during D0

The kernel-input-spec doc must answer these before D1 starts coding:

1. **Manifest format.** SMT-LIB comments vs separate `.json` sidecar vs
   embedded `(set-info …)` annotations. Comments are simplest; settle
   on `;; manifest: <key>=<value>` style.

2. **State Datatype vs flat fields.** Plan says flat. The manifest
   resolves "which fields belong to state."

3. **`_state` wiring.** Two options:
   - The SMT-LIB declares `_state.x` as a free Int; the kernel asserts
     `(assert (= _state.x <prev>))` before each solve.
   - The SMT-LIB has a placeholder for the kernel to substitute.

   Lean: option 1 (kernel-asserts). Keeps the .smt2 stateless.

4. **`last_results` wiring.** Same shape as `_state` — kernel asserts
   `(assert (= last_results (seq.++ (seq.unit r0) (seq.unit r1) …)))`
   before each solve. Need to confirm the existing `encode_ast.rs`
   support for SeqEnum literal construction extends here.

5. **Tick-0 initial state.** Either:
   - The FSM's body must have a `is_first_tick ? init : continued` shape
     and the kernel pre-asserts `is_first_tick = true` on tick 0.
   - The SMT-LIB has an initial block the kernel asserts only on tick 0.

   Lean: option 1 (FSM-author-controlled). Same as the existing
   runtime's behavior with `is_first_tick`.

6. **Multiple `Exit`s in one Seq.** Plan says first wins. Document
   explicitly.

7. **Failure of the manifest parse.** Kernel exits 3 (internal error)
   with a clear message naming the malformed line.

8. **Multi-line stdin perf.** Each `ReadLine` is one effect, one tick,
   one Z3 solve. For 1M lines, that's 1M solves. Acknowledged perf
   ceiling. Future iteration may add `ReadLines(n) → Seq(Result)`.

## Deferred to later iterations

- **Effect toposort.** Instead of explicit `++` chaining, declare
  dependencies between Effects and let the runtime topologically sort
  them. Cuts boilerplate but adds a new pre-translation pass. Iter 3+.
- **Self-hosted compiler stages.** Lexer-in-Evident, then parser, then
  AST-to-SMT-LIB. Each replaces a Rust stage. Iter 2+.
- **UNSAT-core diagnostics.** When a tick goes UNSAT, currently kernel
  just prints "UNSAT, exit 2." Adding unsat-core extraction + Evident-
  source localization is a real feature, ~200 LOC, deferred.
- **`default x = expr` language feature.** The conjunctive-composition
  trap on shared fields is sidestepped by single-writer + ternary +
  the `++` composition pattern. Defaults remain a real gap for other
  use cases; revisit when we hit them in practice.
- **Multi-FSM scheduling, async event sources.** Subsumed by "many
  effects per tick" + kernel loop. No scheduler needed.
- **`Spawn(fsm_name, seed)` effect.** Lets a tick spawn a fresh FSM
  instance. Possible v2 effect; not in v1.

## Build order

Revised (per critique):

1. **D0 first** — write the input spec doc. Pure text. Unblocks D1+D2.
2. **D3 second** — `stdlib/kernel.ev`. Smallest, lowest risk. Validates
   the language can express the Effect/Result enums end-to-end.
3. **D1 + D2 in parallel** — kernel and emit are independent given the spec.
4. **D4 + D5** — fixtures + driver + test.sh wiring.

## Rough sizing (revised)

| Deliverable | LOC | Risk |
|---|---:|---|
| D0 input spec | ~1 page text | Low |
| D3 stdlib/kernel.ev | 50-100 Evident | Low |
| D2 emit | ~300 Rust | Low (extends translate/) |
| D1 kernel | 800-1200 Rust | **Medium** (libffi + manifest + Effect dispatch) |
| D4 fixtures + driver | 50 Python + 5 .ev | Low |
| D5 test.sh phase | 30 bash | Low |
| **Total** | **~1100-1500 Rust new, ~100 Evident, ~80 driver** | |

After iteration 1: existing Rust runtime ~9.8K LOC (untouched). Kernel
is new (additive). Iteration 2 starts the self-hosting that *reduces*
the Rust runtime.
