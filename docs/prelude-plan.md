# The Prelude — plan and acceptance criteria

## Status (auto-loop, updated 2026-05-31)

Landed on `tiny-runtime`:

- **M1** ✅ — hello world via LibCall
- **M2** ✅ — `__mem__` primitives in ffi.py
- **M3** ✅ — Stack FTI (relational push/pop, supported-transition assertion)
- **M4** ✅ — Queue FTI (FIFO, same shape as Stack with `tail` for dequeue)
- **M7** ✅ — `++` operator + 7 seq idioms (head/last/len/init/tail/unit/empty)

Design landed, implementation pending:

- **M5** ⚙ — Z3 FTI + Formula datatype. Design in `docs/fti-z3.md`. Implementation in progress on `prelude-m5-z3`.
- **M6** ⚙ — Set-theoretic + quantifier Formula extensions. Design in `docs/fti-z3-m6-extensions.md`.

Not yet started:

- **M8** — Real demo (sudoku or zebra). Depends on M5+M6.

## What the prelude is

The **prelude** is the body of Evident code (`.ev` files in `prelude/`)
that establishes the runtime environment for every Evident program.
It is not a "standard library" in the conventional sense — it contains
no user-facing algorithms, no convenience utilities, no domain code.
It contains only what is required for Evident programs to function:
the FTI declarations that materialize external state, the symbolic
representation of Z3 formulas, and the parser/transpiler support for
the relational idioms users need.

A separate user-facing utility layer (call it `lib/` or `stdlib/`) may
come later. The prelude is below that.

This plan operates under the framing committed to in
[`CLAUDE.md`](../CLAUDE.md): Evident is relational constraint
programming. No function calls, no execution order, no `x_next`. The
four structures (`type`, `claim`, `fsm`, `fti`) are the only
abstractions. FTIs are specialized types whose state-pair variables
are materialized against external systems via libcall; FTIs own their
foreign state, do not re-read it each tick, and are composed into
host `fsm`s by simple declaration. The composing FSM constrains FTI
variables relationally (`s.contents = _s.contents ++ [42]`) — never
by command dispatch.

## The two halves the prelude must cover

The prelude provides two families of capabilities. They share the
FTI mechanism underneath but serve different purposes.

| Half | What it is | Examples |
|---|---|---|
| **External-state FTIs** | Make external mutable state machines available to Evident FSMs as ordinary state variables. The FTI's libcalls materialize Evident's variables in external memory. | Stack, Queue, File, Mutex, GPU buffer |
| **Z3 FTI + Formula datatype** | Make Z3 itself available to Evident programs as an FTI. Build symbolic formulas as ordinary Evident values; the Z3 FTI materializes them via libcall and reports back the solver's results. | The `Z3` FTI; the `Formula` datatype |

Both halves use the same FTI mechanism. The first half makes data
structures (whose foreign state is in OS memory) accessible. The
second half makes Z3 itself (whose foreign state is in Z3's own
handle tables) accessible.

The composing FSM never sees libcalls. It declares variables of FTI
types, constrains them relationally, and the FTIs handle the
synchronization.

## Foundational decision — memory primitives

External-state FTIs need a way to actually read and write external
bytes. `ffi.py` can dispatch arbitrary C function calls, but it has
no primitive for "load an int from this address" or "store an int at
this address." Libc doesn't expose these as named functions; the
natural way in C is to dereference a pointer, which has no FFI shape.

This is the foundational gap. Resolution is a **bug-fix-shaped
extension to `ffi.py`** — adding four small primitives, dispatched
under a special library name `__mem__`:

```python
mem_alloc(size_bytes)   → addr            # malloc wrapper
mem_load_long(addr)     → long value      # ctypes.c_long.from_address
mem_store_long(addr, v)                   # ctypes.c_long.from_address = v
mem_free(addr)                            # free wrapper
```

These are not "features." They are missing primitive operations that
make `libcall`-mediated external memory possible at all. Without
them, FTIs cannot materialize anything beyond simple integers
returned from existing C functions. The runtime stays at "trampoline
+ libcall + these four memory primitives." ~20 lines of Python
addition.

## Milestones

Each milestone delivers something that runs end-to-end and is
acceptance-tested by a concrete example program.

### M1: Hello world via LibCall (no prelude code yet)

**Goal.** Confirm the existing transpiler emits LibCall effects
correctly, the runtime dispatches them, and a real C function (puts)
runs end-to-end via .ev syntax.

**Deliverable.** `examples/hello.ev`:

```
fsm hello()
    effects = match is_init:
        true  => [LibCall("libc", "puts", "i(s)",
                          [ArgStr("hello, world")],
                          "", "")]
        false => []
```

**Acceptance test.** `python3 src/main.py examples/hello.ev` prints
`hello, world` and exits cleanly.

**Estimated size.** ~10 lines of Evident.

**Unlocks.** The is_init guard pattern, ArgStr usage, libcall sig
conventions. Every later FTI uses these patterns inside its body.

### M2: Memory primitives in ffi.py + raw memory test

**Goal.** Land the four memory primitives. Prove they work end-to-end
with no prelude wrappers.

**Deliverable (Python).** Four functions added to `src/ffi.py`,
exposed as a special library `__mem__`: `mem_alloc`, `mem_load_long`,
`mem_store_long`, `mem_free`.

**Deliverable (Evident).** `examples/mem_raw.ev` — alloc, store 42,
load, free. Verifies that load returns 42.

**Acceptance test.** Runs, exits cleanly, prints something showing
the round-trip worked.

**Estimated size.** ~20 lines of Python in ffi.py; ~30 lines of
Evident in the test.

**Unlocks.** Stack FTI, Queue FTI, every external memory FTI.

### M3: Stack FTI (relational, no commands)

**Goal.** First FTI. A Stack of Ints backed by external memory.
Proves the FTI declaration shape works, the materialization
mechanism works, and the composing FSM uses pure relational syntax
— no command ports.

**Note on parser support.** The bootstrap parser does not yet
recognize `fti` as a keyword. Bug-fix-shaped change: add `fti` as
a keyword; lower it the same as `fsm` (since FTI is `fsm`-shaped at
the SMT-LIB level — the materialization libcalls are emitted from
the FTI body just like any other effect). ~10 lines.

**Deliverable.** `prelude/stack.ev`:

```
fti Stack(T)
    base ∈ Int               ; externally-allocated region start
    contents ∈ Seq(T)        ; the logical stack contents

    ; init (tick 0): allocate external region, _contents starts empty
    ; tick end: emit libcalls so that the bytes at `base` reflect
    ;           the current `contents` value
    ; (FTI body details belong in the implementation, not here)
```

**Usage example** — push three values, pop them in order:

```
fsm push_three()
    s ∈ Stack(Int)
    phase ∈ {0, 1, 2, 3}

    phase = match _phase:
        0 => 1
        1 => 2
        2 => 3
        _ => _phase

    s.contents = match _phase:
        0 => _s.contents ++ [10]
        1 => _s.contents ++ [20]
        2 => _s.contents ++ [30]
        _ => _s.contents
```

Notice: no `Push(...)` constructor, no `cmd` port. The composing FSM
asserts the *relation* `s.contents = _s.contents ++ [10]`. The Stack
FTI sees that relation and materializes the difference by emitting
libcalls to write to external memory.

For popping: `s.contents = init(_s.contents)` (drop last element).
For peeking: `top = last(_s.contents)` — just read.

**Acceptance test.** `examples/stack_basic.ev` — push 1, 2, 3; pop
them; verify LIFO order via prints.

**Estimated size.** Parser change ~10 lines; Stack FTI ~80 lines;
test ~40 lines.

**Unlocks.** Queue FTI; LR parsers (much later); any PDA-shaped
program.

### M4: Queue FTI

**Goal.** FIFO queue. Same relational shape as Stack — composing
FSM asserts `q.contents = _q.contents ++ [x]` to enqueue,
`q.contents = tail(_q.contents)` to dequeue.

**Deliverable.** `prelude/queue.ev` with Queue FTI.

**Acceptance test.** Enqueue 1, 2, 3; dequeue them; verify FIFO order.

**Estimated size.** ~80 lines of Evident.

### M5: Z3 FTI + Formula datatype

**Goal.** Bring Z3 itself into Evident programs as an FTI. The
program declares Z3 formulas symbolically using a `Formula` datatype;
the Z3 FTI materializes them by emitting libcalls to Z3's C API.

This is the *Architecture B* path from
[`docs/runtime-architecture.md`](runtime-architecture.md): programs
build a growing constraint model and solve it. The Z3 FTI is what
makes that path expressible in Evident.

**Deliverable.** `prelude/formula.ev` and `prelude/z3.ev`:

```
; The symbolic representation of a Z3 formula.
type Formula =
    | IntLit(value ∈ Int)
    | BoolLit(value ∈ Bool)
    | Var(name ∈ String, sort_name ∈ String)
    | Eq(l ∈ Formula, r ∈ Formula)
    | Add(l ∈ Formula, r ∈ Formula)
    | Sub(l ∈ Formula, r ∈ Formula)
    | And(args ∈ Seq(Formula))
    | Or(args ∈ Seq(Formula))
    | Not(arg ∈ Formula)
    | Lt(l ∈ Formula, r ∈ Formula)
    | Le(l ∈ Formula, r ∈ Formula)
    ; ... etc.

type SatResult = | Unknown | Sat | Unsat

; The Z3 FTI. The composing FSM declares one variable of this type
; and constrains `formulas` to be the assertions it wants Z3 to know.
fti Z3
    formulas ∈ Seq(Formula)   ; assertions
    sat ∈ SatResult           ; result of latest check
    model ∈ String            ; SMT-LIB representation of the model

    ; init: libcalls to Z3_mk_config, Z3_mk_context, Z3_mk_simple_solver.
    ; tick end: emit libcalls to push any new entries in `formulas`
    ;           to Z3's solver, call Z3_solver_check, populate sat
    ;           and model from the result.
```

**Usage example** — solve `x + y = 8, x = 3, y = 5`:

```
fsm find_sum()
    z ∈ Z3

    z.formulas = match is_init:
        true => [
            Eq(Var("x", "Int"), IntLit(3)),
            Eq(Var("y", "Int"), IntLit(5)),
            Eq(Var("z", "Int"), Add(Var("x", "Int"), Var("y", "Int")))
        ]
        false => _z.formulas

    effects = match z.sat:
        Sat   => [LibCall("libc", "puts", "i(s)",
                          [ArgStr("SAT — see model")], "", "")]
        Unsat => [LibCall("libc", "puts", "i(s)",
                          [ArgStr("UNSAT")], "", "")]
        _     => []
```

Two-tick latency: tick 0 asserts the formulas; tick 1 reads `z.sat`
(now populated by the FTI's materialization at end of tick 0).
Documented as part of the libcall result-binding pattern.

**Acceptance test.** Runs, prints `SAT — see model`. Manual check of
the model output confirms x=3, y=5, z=8.

**Estimated size.** Formula datatype ~80 lines; Z3 FTI body ~150
lines; test ~30 lines.

**Unlocks.** Set-theoretic Formula extensions; quantifier extensions;
real demos.

### M6: Set-theoretic and quantifier extensions to Formula

**Goal.** Extend the Formula datatype with the Z3 set-theoretic
operations and quantifiers. Evident is meant primarily as a set
theory language, so these are the idiomatic constructors.

**Deliverable.** Additional variants in `prelude/formula.ev`:

```
type Formula =
    ; ...all the M5 constructors, plus:
    | SetEmpty(sort_name ∈ String)
    | SetFull(sort_name ∈ String)
    | SetAdd(set ∈ Formula, elem ∈ Formula)
    | SetDel(set ∈ Formula, elem ∈ Formula)
    | SetUnion(l ∈ Formula, r ∈ Formula)
    | SetIntersect(l ∈ Formula, r ∈ Formula)
    | SetDifference(l ∈ Formula, r ∈ Formula)
    | SetComplement(set ∈ Formula)
    | SetMember(elem ∈ Formula, set ∈ Formula)
    | SetSubset(a ∈ Formula, b ∈ Formula)
    | Forall(var_name ∈ String, set ∈ Formula, body ∈ Formula)
    | Exists(var_name ∈ String, set ∈ Formula, body ∈ Formula)
```

The Z3 FTI's materialization is extended to handle these via the
corresponding Z3 C API functions (`Z3_mk_set_union`, etc.).

**Acceptance test.** A program that builds two sets {1, 2, 3} and
{2, 3, 4}, computes their intersection symbolically, asserts a
membership query (`x ∈ A ∩ B`), gets SAT with a satisfying x.

**Estimated size.** Formula extensions ~60 lines; Z3 FTI extensions
~100 lines; test ~40 lines.

### M7: Sequence idioms in user code

**Goal.** Make Seq operations ergonomic in user code (inside claims
and FSM bodies). These are NOT external memory operations and don't
go through any FTI — they're relations among Z3 Seq values used
directly in constraints.

**Required parser/transpiler bug fixes:**
- `++` as a binary operator → emits `(seq.++ a b)`
- Built-in identifiers `head`, `last`, `len`, `init`, `tail`,
  `empty`, `unit` recognized as built-in calls, lowered to
  `seq.nth`, `seq.extract`, `seq.len`, etc.

**Deliverable.** Parser/transpiler bug fixes plus `prelude/seq.ev`
with documentation (most of the file is comments — these are
language idioms, not library code).

```
; head(s)  = first element of s, used as a value in constraints
; last(s)  = last element of s
; len(s)   = length of s
; init(s)  = s with last element dropped
; tail(s)  = s with first element dropped
; s ++ t   = concatenation
; empty(T) = the empty sequence of element type T
; unit(x)  = the singleton sequence [x]
```

**Acceptance test.** A claim that constrains a Seq's head, last, and
length, and verifies the solver finds a valid Seq.

**Note.** These idioms are also what M3/M4 Stack/Queue FTIs *expect*
to be available — the composing FSM writes `s.contents = _s.contents
++ [x]` using the very `++` and `Seq` machinery defined here. M7 may
actually need to land *before* M3/M4 in practice, even though
conceptually it sits later. Resolve by doing M7 first if Stack/Queue
implementation forces the issue.

**Estimated size.** Parser/transpiler bug fixes ~30 lines of Python;
prelude doc + tests ~60 lines of Evident.

### M8: A real demo

**Goal.** End-to-end proof the prelude is useful. Solves a small
constraint puzzle by building a Z3 model from Evident.

**Deliverable.** `examples/sudoku4.ev` (a 4x4 Sudoku) or
`examples/zebra.ev` (the classic puzzle). Pick whichever is smaller
when actually written.

**Acceptance test.** Runs, prints the solution, finishes in seconds.

**Estimated size.** ~150 lines of Evident.

**Unlocks.** Confidence that the prelude actually serves what Evident
is for.

## Build order and dependency graph

```
M1 (hello)  ──┐
              ├──→ M3 (Stack) ──→ M4 (Queue)
M2 (memory) ──┘                              ╲
              ╱──→ M7 (seq idioms) ──┐        ╲
M1 (hello)  ─┤                       ├──────→ M8 (demo)
              ╲──→ M5 (Z3+Formula) ──┴──→ M6 (set/quantifier)
```

Two parallel tracks:

- **External memory track** (M1 → M2 → M3 → M4). FTIs for Stack and
  Queue, backed by external memory. Unlocks PDA-class FSMs.
- **Z3 constraint track** (M1 → M5 → M6). Z3 FTI and the Formula
  datatype. Unlocks Architecture B — Evident programs that build
  constraint models programmatically.

M7 (sequence idioms) is a parser/transpiler enhancement that
benefits both tracks. Schedule it where convenient; M3 may need it
as a prerequisite.

M8 joins everything for the demo.

## Grammar gaps and bug-fix-shaped extensions

| Bug fix | What it is | Lines | Milestone |
|---|---|---|---|
| Memory primitives in `ffi.py` | `mem_alloc` / `load_long` / `store_long` / `free` under library name `__mem__` | ~20 | M2 |
| `fti` keyword in parser/transpiler | Recognize FTI declarations; lower the same as `fsm` | ~10 | M3 |
| `++` binary operator | Lower to `(seq.++ a b)` | ~5 | M7 |
| Seq accessor identifiers | `head`, `last`, `len`, `init`, `tail`, `empty`, `unit` recognized as built-in calls | ~25 | M7 |

Total Python additions: ~60 lines. After M7 the bootstrap is again
frozen until the next foundational gap is discovered.

## Conventions to lock in early

**Naming.**
- snake_case for claims, fsms, ftis, variables.
- FTI types use PascalCase as simple nouns for the thing the state
  machine represents: `Stack`, `Queue`, `File`, `Mutex`, `Z3`. No
  `Handle`, `Access`, `State` suffixes. If a name needs a suffix to
  convey "this is a stateful thing," the FTI shouldn't exist — it's
  just a variable.

**File layout.**
- `prelude/stack.ev` — Stack FTI
- `prelude/queue.ev` — Queue FTI
- `prelude/formula.ev` — Formula datatype
- `prelude/z3.ev` — Z3 FTI
- `prelude/seq.ev` — sequence idiom documentation

**FTI design conventions.**

- An FTI declares state-pair variables (`base`, `contents`, `sat`,
  etc.) that get materialized via libcall at tick boundaries. It
  does NOT have "command ports" or "cmd" enums. The composing FSM
  asserts relations over the FTI's state-pair variables; the FTI
  detects the difference between `_x` and `x` and emits the
  appropriate libcalls.

- The FTI is the sole writer to its foreign state. The runtime
  carries `x` forward as `_x` between ticks — the FTI does NOT
  re-read external memory at tick start. If the foreign state could
  be modified by other parties, that's not an FTI; that's the Actor
  pattern.

- An FTI's init libcalls (those guarded by `is_init`) are the only
  ones that *read* external state — to initialize the FTI's variables
  to whatever the foreign system was already holding. After that,
  every tick only *writes*.

**Composition.**

- An FSM declares `q ∈ Queue(Int)` to bring a Queue FTI's variables
  into its body (namespaced under `q`). The transpiler handles the
  namespacing and the constraint composition.

- Multiple FTIs can be composed into one FSM by declaring multiple
  variables of FTI types. The runtime sees one combined body.

- FTIs do not compose with each other directly. They compose with
  `fsm`s that hold them.

**The two-tick latency for libcall results.**

When an FTI's tick-end libcall produces a value (Z3's sat result, a
file's bytes read, etc.), that value is available to the composing
FSM on the *next* tick. This is a fundamental consequence of the
state-pair model: the libcall fires after the tick's solve, so the
value can't appear until next tick's `_x` carries it forward.

For programs that need to react to libcall results, this means a
two-tick pattern: tick N "asks" (sets formulas), tick N+1 "sees"
(reads sat). Standard FSM idiom; users get used to it.

**Sigs.** Stick to `i/l/d/s/v` from `ffi.py`. Pointers are `l`
(8 bytes). Z3 AST/sort/solver handles are pointers, so they're `l`.
Out-parameters and arrays are not yet supported — separate bug-fix-
shaped extension when needed.

## Out of scope for the prelude

- **Self-hosted parser.** The bootstrap parser is in Python; rewriting
  it in Evident is a separate later effort.
- **User-facing utilities.** Permutations, sorting, hash tables, math
  libraries, anything algorithm-shaped — these belong in a future
  `lib/` or `stdlib/`, not the prelude.
- **Multi-threading and Actor pattern.** Single-threaded for v1.
  Pthread libcalls can be added later via the Actor pattern (separate
  `fsm`s communicating over channels), not as FTIs.
- **Sockets, networking.** External-party writers; Actor-shaped, not
  FTI-shaped. Defer.
- **GUI, audio, graphics.** Domain libraries; not the prelude.
- **Performance work.** Correctness first. The bootstrap is
  interpreted; the JIT-compilation of FSMs is a separate future
  project that the prelude does not block.

## Total estimated size

- Python bootstrap additions: ~60 lines across four bug fixes
- Evident prelude code: ~750 lines across 5 files
- Evident test/example code: ~350 lines
- **Total prelude work: ~1100 lines of Evident, ~60 lines of Python.**

Achievable. Each milestone is small enough to verify in isolation.

## How to know we're done

When all eight milestones pass their acceptance tests, the prelude
is v1. The single best signal of health: an Evident programmer can
build a Z3 model, solve it, and read the answer — by declaring `z ∈
Z3`, asserting `z.formulas = [...]`, and reading `z.sat` — *without
writing any libcall by hand*. Everything goes through the FTIs.

The same bar applies to data: an Evident programmer can manage
external state — push/pop stacks, enqueue/dequeue queues, read/write
files — by declaring `s ∈ Stack(Int)` (or similar) and asserting
relations like `s.contents = _s.contents ++ [42]`. No libcall soup
visible at the user level.

That's the prelude's reason for existing.
