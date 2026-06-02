# Evident

A relational constraint programming language whose programs are sets of
constraints over named variables, and whose runtime is Z3 finding a
satisfying assignment.

This is the bootstrap implementation: a ~700-line Python runtime
(trampoline + libcall) that runs Evident programs (`.ev` files) by
transpiling to SMT-LIB and ticking the resulting body through Z3 until
it halts.

## Quick start

```bash
python3 src/main.py examples/sum_is_eight.ev
# x = 3
# y = 5
# z = 8
```

That program reads:

```
claim sum_is_eight()
    x ∈ {0..10}
    y ∈ {0..10}
    z ∈ {0..20}
    z = x + y
    x = 3
    y = 5
```

The variables `x`, `y`, `z` are declared with set-membership constraints;
the three equations relate them. The solver finds `z = 8`.

## What Evident is, briefly

Programs are built from four structures:

- **`type`** — a way to group variables (like a record, but not a class).
- **`claim`** — constraints over variables; a predicate. Composes by being
  merged into other contexts.
- **`fsm`** — a constraint system with the state-pair convention: every
  parameter `x` produces both `_x` (previous tick) and `x` (this tick),
  and the body relates them. The runtime ticks until the state stabilizes.
- **`fti`** — a Foreign Type Interface. A specialized type whose state-pair
  variables are *materialized* against an external system through libcall.
  Stacks, queues, files, Z3 itself — all FTIs.

Programs are **relational**, not procedural. `head(s) = 1` is a constraint
that the first element of `s` equals 1, not a function call. There is no
execution order — Z3 solves all constraints simultaneously each tick.

See [`CLAUDE.md`](CLAUDE.md) for the full operational rules; the
"This is not a language you know" section explains the framing in detail.

## What's in the repo

```
src/                Bootstrap runtime (5 files, ~830 lines of Python)
    main.py         CLI entry; loads prelude, parses, transpiles, runs
    runtime.py      The trampoline — solves bodies until halt
    ffi.py          libcall via ctypes; `__mem__` primitives
    parser.py       Hand-written recursive-descent for the Evident grammar
    transpile.py    AST → SMT-LIB; FTI registry + namespace inlining

prelude/            FTI declarations (Evident code)
    stack.ev        Stack FTI (push, pop, no-op)
    queue.ev        Queue FTI (enqueue, dequeue, no-op)
    z3.ev           Z3 FTI (pending)

examples/           Programs that exercise the runtime
    sum_is_eight.ev    Three-variable constraint claim
    counter.ev         Simple FSM bounded counter
    hello.ev           libcall to puts
    mem_raw.ev         __mem__ round-trip test
    seq_test.ev        Seq idioms (head, last, len)
    stack_basic.ev     Stack push/pop through external memory
    stack_unsat.ev     Stack rejects unsupported transitions
    queue_basic.ev     Queue enqueue/dequeue through external memory
    queue_unsat.ev     Queue rejects unsupported transitions

docs/               Architecture decisions and design docs
    runtime-architecture.md      The whole runtime story
    seq-idioms.md                head/last/len/++ etc.
    fti-composition.md           How FTIs compose into FSMs
    fti-z3.md                    Z3 FTI design (M5)
    fti-z3-m6-extensions.md      Set/quantifier extensions (M6)
    m8-demo-sketch.md            Closing 4x4 Sudoku demo
    prelude-plan.md              The prelude milestones + status
    keeping-it-minimal.md        Philosophy
```

## Architecture

The runtime is intentionally tiny. From `CLAUDE.md`:

> The runtime is two things: a **trampoline** that runs an SMT-LIB FSM
> body to halt, and **libcall** that bridges to any C library via ctypes.
> Everything else — Z3 access, data structures, multi-FSM patterns,
> JIT — is library code that uses libcall.

The Python in `src/` is **frozen** after the bootstrap. New features go in
Evident (`.ev` files in `prelude/`), not Python.

See [`docs/runtime-architecture.md`](docs/runtime-architecture.md) for
the long version.

## Running examples

```bash
python3 src/main.py FILE.ev               # parse, transpile, run
python3 src/main.py --emit-smt FILE.ev    # just emit the SMT-LIB
```

Verify everything works:

```bash
for f in examples/*.ev; do
    echo "==> $f"; python3 src/main.py "$f" 2>&1 | tail -3; echo
done
```

## Project status

See [`docs/prelude-plan.md`](docs/prelude-plan.md) for the milestone
list and what's landed.

The bootstrap (runtime + parser + transpiler) and FTIs M3 (Stack) and
M4 (Queue) are landed. M5 (Z3 FTI) and M6 (set/quantifier extensions)
are designed; implementation in progress. M8 (real demo) is the closer.
