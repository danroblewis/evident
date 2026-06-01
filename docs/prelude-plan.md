# The Prelude — plan and acceptance criteria

## What the prelude is

The **prelude** is the body of Evident code (`.ev` files in `prelude/`)
that establishes the runtime environment for every Evident program.
It is not a "standard library" in the conventional sense — it contains
no user-facing algorithms, no convenience utilities, no domain code.
It contains only what is required for Evident programs to function:
the Z3 bindings, the FTI declarations for external memory, the
set-theoretic primitives, and the idiomatic operations on sequences
and sets.

A separate user-facing utility layer (call it `lib/` or `stdlib/`) may
come later. The prelude is below that. The distinction is intentional
and the names should not be confused.

This plan operates under the framing committed to in
[`CLAUDE.md`](../CLAUDE.md): Evident is relational constraint
programming. No function calls, no execution order, no `x_next`. The
four structures (`type`, `claim`, `fsm`, `fti`) are the only
abstractions. Anyone writing prelude code who finds themselves
reaching for imperative or functional patterns has reverted and
should stop.

## The two halves the prelude must cover

The prelude provides two distinct families of operations. They do not
mix.

| Half | What it is | Mechanism | Examples |
|---|---|---|---|
| **Constraint operations** | Relations among values inside the Z3 model | Value expressions in claims | `head(s)`, `len(s)`, `S ∪ T`, `x ∈ S`, `∀ y ∈ S. P(y)` |
| **External-state FTIs** | Bridges to mutable state machines outside the model | Libcall effects from FTI bodies | Stack, Queue, FileHandle, Mutex, Channel |

Constraint operations are relations. They don't mutate anything. They
constrain how variables relate. When the body of a claim says
`head(s) = 1`, it does not "fetch the head and compare." It declares
the relation "the head element of s equals 1," and the solver finds
an `s` consistent with that and every other constraint in scope.

External-state FTIs wrap actual external machines. A Stack FTI does
not put a stack inside Z3's model. It models the *visible state* of
an external stack (the current top value, the current depth) in Z3
variables, and emits libcalls each tick to keep external memory and
the modeled view synchronized. The stack itself lives in OS-managed
memory, accessed through libcall.

A program that needs a parser builds it as an `fsm` that composes
with a Stack FTI. The parser's tick decides shift vs reduce; the
Stack FTI's tick performs the corresponding external memory mutation.
Z3 sees only bounded per-tick state — the rest is external.

## Foundational decision — memory primitives

External memory FTIs need a way to actually read and write external
bytes. The current `ffi.py` can dispatch arbitrary C function calls,
but it has no primitive for "load an int from this address" or
"store an int at this address." Libc itself doesn't have these as
named functions; the natural way in C is to dereference a pointer,
which has no FFI shape.

This is the foundational gap. Resolution is a **bug-fix-shaped
extension to `ffi.py`** — adding four small primitives:

```python
mem_alloc(size_bytes)   → addr            # malloc wrapper
mem_load_long(addr)     → long value      # ctypes.c_long.from_address
mem_store_long(addr, v)                   # ctypes.c_long.from_address = v
mem_free(addr)                            # free wrapper
```

These are not "features." They are the missing primitive operations
that make `libcall`-mediated external memory possible at all. Without
them, the language cannot express PDA-class FSMs, which CLAUDE.md
identifies as part of v1. The runtime stays at "trampoline + libcall +
these four memory primitives." ~20 lines of Python addition. Document
the addition explicitly in the commit; this is the kind of foundational
change the freeze permits as a bug fix.

Once these exist, the prelude implements Stack, Queue, and every
other external memory FTI without further runtime changes.

## Milestones

Each milestone delivers something that runs end-to-end and is
acceptance-tested by a concrete example program. Estimated sizes are
rough. Each milestone unlocks the next.

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
conventions. Every later FTI uses these.

### M2: Memory primitives in ffi.py + raw memory test

**Goal.** Land the four memory primitives. Prove they work end-to-end
with no prelude wrappers.

**Deliverable (Python).** Four functions added to `src/ffi.py`:
`mem_alloc`, `mem_load_long`, `mem_store_long`, `mem_free`. They are
exposed as a special intrinsic library — calling them looks like
`libcall("__mem__", "alloc", ...)` or similar. Pick a convention and
document it.

**Deliverable (Evident).** `examples/mem_raw.ev`:

```
fsm raw_mem_test(addr ∈ Int, value ∈ Int)
    ; alloc, store 42, load, free — all via libcall to __mem__
    ; check that value == 42 after the load
    ...
```

**Acceptance test.** Runs, prints `value = 42`, exits.

**Estimated size.** ~20 lines of Python in ffi.py; ~25 lines of
Evident in the test.

**Unlocks.** Stack FTI, Queue FTI, every other external memory FTI.

### M3: Stack FTI

**Goal.** First real FTI. A Stack of Ints backed by external memory.
Proves the FTI declaration shape works, the libcall composition
works, and the state-pair convention extends correctly to FTIs.

**Note on syntax.** The bootstrap parser does not currently have an
`fti` keyword. We have two paths:

- (a) Bug-fix the parser to add `fti` as a keyword. ~10 lines of
  parser + transpiler change. Recognized as bug-fix-shaped because
  FTIs are v1.
- (b) Write the FTI as an `fsm` with a doc comment marking it as
  an FTI for now. Same SMT-LIB output; less ergonomic.

Pick (a). The parser change is small and lets prelude code be
honest about what's an FTI vs what's a regular FSM.

**Deliverable.** `prelude/stack.ev`:

```
fti Stack(base ∈ Int, sp ∈ Int, top ∈ Int, cmd ∈ StackCmd)
    ; base is the malloc'd region start (set once at init)
    ; sp is the current depth
    ; top is the current top value (or 0 if empty)
    ; cmd is the operation to perform this tick — Push(v), Pop, or Noop
    ; transition relation handles each cmd; emits libcalls accordingly
    ...
```

**Acceptance test.** `examples/stack_basic.ev` — a small driver FSM
that creates a Stack, pushes 1, 2, 3, pops them, verifies LIFO order.

**Estimated size.** Parser change ~10 lines; Stack FTI ~80 lines;
test ~30 lines.

**Unlocks.** Queue FTI; LR parser (in a much later milestone); any
PDA-shaped program.

### M4: Queue FTI

**Goal.** FIFO queue, similar shape to Stack. Demonstrates the FTI
pattern is reusable.

**Deliverable.** `prelude/queue.ev` with Queue FTI.

**Acceptance test.** Push 1, 2, 3; pop them; verify FIFO order.

**Estimated size.** ~80 lines of Evident.

### M5: Set-theoretic Z3 bindings

**Goal.** Wrap Z3's set-theoretic C API. This is the *constraint*
side of the prelude — operations that go inside Z3 models, used in
claims. Distinct from M3/M4 which are external memory FTIs.

**Deliverable.** `prelude/sets.ev`:

```
claim mk_set_sort(elem_sort ∈ Int, sort_handle ∈ Int)
    ; wraps Z3_mk_set_sort via libcall; uses is_init guard pattern

claim mk_empty_set(ctx ∈ Int, sort ∈ Int, set_handle ∈ Int)
    ; wraps Z3_mk_empty_set

claim mk_set_add(ctx ∈ Int, set ∈ Int, elem ∈ Int, result ∈ Int)
    ; wraps Z3_mk_set_add

claim mk_set_union(ctx ∈ Int, a ∈ Int, b ∈ Int, result ∈ Int)
claim mk_set_intersect(ctx ∈ Int, a ∈ Int, b ∈ Int, result ∈ Int)
claim mk_set_member(ctx ∈ Int, elem ∈ Int, set ∈ Int, result ∈ Int)
claim mk_set_subset(ctx ∈ Int, a ∈ Int, b ∈ Int, result ∈ Int)
; ... etc
```

**Important framing.** Each of these is a claim that *relates* its
parameters via libcall effects. They are NOT function calls. The
calling code declares the result variables and merges the claim's
constraints (which include the libcall effects) into its body. The
"return value" is the handle pinned by the libcall.

The handles are opaque Z3 AST pointers, held as Ints. Pass them to
later set operations. The user reasons about them by name, not by
following execution.

**Acceptance test.** `examples/set_intersect.ev` — declares a Z3
context, builds two sets {1, 2, 3} and {2, 3, 4}, computes their
intersection, asks Z3 to enumerate members, prints {2, 3}.

**Estimated size.** ~150 lines of Evident; ~50 lines of test.

**Unlocks.** Quantifiers (M6), full set-theoretic programs.

### M6: Quantifier wrappers

**Goal.** Add the `∀ x ∈ S. P(x)` and `∃ x ∈ S. P(x)` constructs.
These are central to set-theoretic programming and Evident's stated
focus.

**Deliverable.** `prelude/quantifiers.ev`:

```
claim mk_forall_in(ctx ∈ Int, var_name ∈ String, set ∈ Int,
                   body ∈ Int, result ∈ Int)
    ; wraps Z3_mk_forall

claim mk_exists_in(...)
    ; wraps Z3_mk_exists

claim mk_membership(ctx ∈ Int, elem ∈ Int, set ∈ Int, result ∈ Int)
    ; wraps Z3_mk_set_member (already in M5; here for completeness)
```

**Acceptance test.** A program that asserts `∀ x ∈ {1,2,3}. x > 0`
and checks SAT.

**Estimated size.** ~80 lines of Evident.

### M7: Solver lifecycle + SMT-LIB round-trip

**Goal.** Full solver bindings. Create, assert, check, model, to-string.

**Deliverable.** `prelude/solver.ev`:

```
claim mk_solver(ctx ∈ Int, solver_handle ∈ Int)
claim solver_assert(ctx ∈ Int, solver ∈ Int, formula ∈ Int)
claim solver_check(ctx ∈ Int, solver ∈ Int, result ∈ Int)
claim solver_to_string(ctx ∈ Int, solver ∈ Int, out_string ∈ String)
```

**Acceptance test.** An Evident program that builds a small model
via the set bindings, calls solver_check, gets SAT, and prints the
canonical SMT-LIB form via solver_to_string. End-to-end Architecture B
from Evident.

**Estimated size.** ~100 lines of Evident.

### M8: Sequence idioms

**Goal.** Idiomatic operations on Z3 Seq values for use *inside the
constraint model*. These are NOT external memory operations — they
are relations among Seq values used in claims.

**Required parser/transpiler bug fixes:**
- `++` as a binary operator → emit `(seq.++ a b)`
- Built-in identifiers `head`, `last`, `len`, `init`, `tail`, `unit`,
  `empty` → emit the corresponding `seq.*` SMT-LIB form

**Deliverable.** Bug fixes plus `prelude/seq.ev` with documented
relational operations:

```
; head(s) = the first element of s, used as a value in constraints
; last(s) = the last element of s
; len(s)  = the length of s
; s ++ t  = the concatenation
; etc.
```

**Acceptance test.** A claim that constrains a Seq's head, last, and
length, and verifies the solver finds a valid Seq.

**Estimated size.** Parser/transpiler bug fixes ~30 lines of Python;
prelude doc + tests ~60 lines of Evident.

### M9: A real demo

**Goal.** End-to-end proof the prelude is useful for a real problem.
Solves a small set-theoretic puzzle by building a Z3 model from
Evident and solving it.

**Deliverable.** `examples/zebra.ev` (the classic constraint puzzle)
or `examples/sudoku4.ev` (a 4x4 Sudoku). Whichever is smaller.

**Acceptance test.** Runs, prints the solution, finishes in seconds.

**Estimated size.** ~150 lines of Evident.

**Unlocks.** Confidence that the prelude actually works for what
Evident is for.

## Build order and dependency graph

```
M1 (hello)  ──┐
              ├──→ M3 (Stack) ──→ M4 (Queue)
M2 (memory) ──┘
              ┌──→ M5 (sets) ──→ M6 (quantifiers) ──┬──→ M7 (solver) ──┐
M1 (hello)  ──┤                                     │                  │
              │                                     M8 (seq idioms) ───┴──→ M9 (demo)
              └─────────────────────────────────────┘
```

Two roughly parallel tracks:

- **External memory track** (M1, M2, M3, M4) — gets us PDA-class FSMs.
- **Z3 constraint track** (M1, M5, M6, M7, M8) — gets us programmatic
  model building from Evident.

They join at M9.

## Grammar gaps and bug-fix-shaped extensions

This plan reveals four bug-fix-shaped additions to the bootstrap.
Each is small, each is documented as a bug fix in its commit, each
addresses a foundational missing piece (not a feature).

| Bug fix | What it is | Lines | Milestone |
|---|---|---|---|
| Memory primitives in `ffi.py` | `mem_alloc`/`load_long`/`store_long`/`free` as intrinsic libcalls | ~20 | M2 |
| `fti` keyword in parser/transpiler | Recognize FTI declarations; lower same as fsm | ~10 | M3 |
| `++` binary operator | Lower to `(seq.++ a b)` | ~5 | M8 |
| Seq accessor identifiers | `head`, `last`, `len`, etc. recognized as built-in calls | ~20 | M8 |

Total Python additions: ~55 lines. After M8 the bootstrap is again
frozen until the next foundational gap is discovered.

## Conventions to lock in early

**Naming.**
- snake_case for claims, fsms, ftis, variables.
- Z3 binding claims mirror the Z3 C API name with the `Z3_` prefix
  stripped: `Z3_mk_set_sort` → `mk_set_sort`, etc.
- FTI types use PascalCase: `Stack`, `Queue`, `FileHandle`.
- Handle variables that hold Z3 AST/sort/solver pointers use names
  ending in `_handle`: `ctx_handle`, `set_handle`, `solver_handle`.

**File layout.**
- One concept per file in `prelude/`.
- `prelude/mem.ev` — wrappers around the memory primitives (if any
  wrapping is helpful)
- `prelude/stack.ev` — Stack FTI
- `prelude/queue.ev` — Queue FTI
- `prelude/sets.ev` — set-theoretic Z3 bindings
- `prelude/quantifiers.ev` — quantifier wrappers
- `prelude/solver.ev` — solver lifecycle
- `prelude/seq.ev` — sequence idiom documentation (most are built-ins)

**Libcall patterns.** Three canonical patterns, used throughout:

```
; Pattern 1: direct effect emission (single tick)
fsm Greeter()
    effects = match is_init:
        true  => [LibCall(...)]
        false => []

; Pattern 2: result-binding libcall (wrap a C function with output)
fsm get_pid(pid ∈ Int)
    effects = match is_init:
        true  => [LibCall("libc", "getpid", "i()", [], "pid", "")]
        false => []

; Pattern 3: repeated libcalls (FSM ticks each emit one)
fsm read_lines(line ∈ String)
    effects = [LibCall("libc", "fgets", ..., [...], "line", "")]
    ; halt when line is empty or matches an EOF sentinel
```

**Result-binding semantics.** When a libcall emits an `ok_dest`, the
named variable is pinned to the libcall's result starting on the
*next* tick. This means single-shot libcalls take two ticks: one to
emit, one to consume. The pattern handles this automatically via the
state-pair convention; users do not see the two-tick latency unless
they need to.

**Sigs.** Stick to `i/l/d/s/v`. Pointers are `l` (8 bytes on every
platform we care about). Out-parameters and arrays are not yet
supported — when they become genuinely needed, that's a separate
bug-fix-shaped extension to the sig grammar.

## Out of scope for the prelude

- **Self-hosted parser.** The bootstrap parser is in Python; rewriting
  it in Evident is a separate later effort that comes after the
  prelude is stable. It is not part of v1.
- **User-facing utilities.** Permutations, sorting, hash tables, math
  libraries, anything algorithm-shaped — these belong in a future
  `lib/` or `stdlib/`, not the prelude.
- **Multi-threading.** Single-threaded for v1. Pthread libcalls can
  be added later as FTIs (Mutex, Thread, Channel).
- **Networking, GUI, audio, graphics.** Domain libraries; not the
  prelude's concern.
- **Performance work.** Correctness first. The bootstrap is interpreted;
  the JIT-compilation of FSMs to native code is a separate future
  project that the prelude does not block.

## Total estimated size

- Python bootstrap additions: ~55 lines across four bug fixes
- Evident prelude code: ~700 lines (8 files)
- Evident test/example code: ~300 lines
- **Total prelude work: ~1000 lines of Evident, ~55 lines of Python.**

Achievable. Each milestone is small enough to verify in isolation.

## How to know we're done

When all nine milestones pass their acceptance tests, the prelude is
v1. The next plan after this one is the user-facing utility layer
(`lib/`) or the self-hosted parser. Or both, in parallel — the
prelude is foundational; what builds on top is open.

The single best signal that the prelude is healthy: an Evident
programmer can build a Z3 model, solve it, and read the answer back
*without writing any libcall by hand*. Everything goes through the
prelude's bindings, which expose Z3 as a set-theoretic relational
toolkit, not as a C API. That's the bar.
