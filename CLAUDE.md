# CLAUDE.md

Operational rules for future Claude sessions on this repo.

## The Python freeze

The Python code in `src/` is **frozen**. After this initial bootstrap
(trampoline + ffi + parser + transpile + CLI), no new features go in
Python. All new work — language features, FTIs, libraries, data
structures, the eventual self-hosted parser — is written in **Evident**
(`.ev` files in `prelude/` and `examples/`).

Exceptions:

- Bug fixes to the bootstrap parser/transpiler. Mark the commit as a
  bug fix and describe the bug.
- Adding a new libcall signature character if a new C primitive type
  is genuinely needed (e.g., pointer-to-array for Z3's n-ary builders).
- Memory-safety / correctness fixes.

If you find yourself wanting to add a feature to `src/`, **stop**.
The feature belongs in Evident. The Python is bootstrap; Evident is
the language.

## This is not a language you know

Evident is **relational constraint programming**. It is not functional.
It is not procedural. It is not object-oriented. It is not logic
programming in the Prolog sense (no unification-with-backtracking),
not dataflow, not reactive, not a tactic language. The mental models
from Haskell, Lisp, Python, Rust, C, JavaScript, OCaml, and the way
people typically write SMT-LIB tactics will **all mislead you**.

What this means in practice:

**There is no execution order.** When you read a body top to bottom,
you are reading a *set* of constraints, not a sequence of steps. Z3
finds an assignment to every variable that satisfies every constraint
*simultaneously*. There is no "first this happens, then that." The
order constraints appear in the text is purely for human readability.

**There are no function calls.** `claim foo(x ∈ Int)` does not define
a callable. It defines a *relation* over `x`. "Using" `foo` in another
context means *merging its constraints* into the surrounding system,
not invoking it as a subroutine. An expression like `head(s)` inside a
constraint is not a function call returning the head — it is a value
expression denoting the head element, used as part of a relation. The
transpiler lowers it to an SMT-LIB term; nothing is "called."

**Variables are not storage.** A variable is a *name* for an unknown
value the solver finds. It does not get "assigned" or "updated."
`x = 3` in a body is not an assignment statement — it is the
constraint "x equals 3." If you have written `let x = ...`, `x := y`,
`mutate x`, or thought "first I compute x, then use it" — you have
made a category error. Stop and rewrite.

**Results are not return values.** Constraints don't have outputs.
They have relations among all variables in scope. A claim that you
might be tempted to call "a function returning a result" is really a
relation `R(a, b, result)` — and "calling" it just merges that
relation into a context where you've already declared `result` as a
variable. The "result" gets a value because the solver finds one for
it, not because anything returned.

**Time only exists in `fsm` / `fti`.** Inside a `claim` or `type`, all
constraints hold at once with no temporal aspect. Time appears only
in the FSM convention: every parameter `x ∈ S` of an `fsm` generates
**two** SMT-LIB constants — `_x` (the previous tick's value) and `x`
(this tick's value). The body's constraints relate them. The runtime
ticks, replacing `_x` for the next tick with the just-found `x`.

**The previous-tick variable is `_x`. The current-tick variable is `x`.
There is no `x_next`. There is no `state_next`. Never write them.
Never propose them. They are wrong, and they will create code that
doesn't work.**

The closest precedents for what Evident is actually doing:

- **SMT-LIB at the modeling level** — Evident is a surface syntax over
  SMT-LIB, not a programming language compiled to it.
- **Mathematical notation** — `let x ∈ S such that P(x)` is the
  natural reading of an Evident binding plus assertion.
- **Constraint Satisfaction Problems** (MiniZinc, AMPL, OPL) — same
  relational shape, same simultaneous-solving semantics.
- **Predicate logic** — claims are predicates; quantifiers are
  quantifiers.

If you reach for Haskell, Lisp, Python, JS, C, Rust, OCaml, or
imperative SMT-LIB tactics patterns — you are reaching wrong. Reach
for predicate logic, set-builder notation, and declarative constraint
satisfaction.

## The four structures

Every Evident program is built from four things and nothing else.

**`type` — variable grouping with constraints.**
A way to bundle related variables that *feels* like a record or class
but is neither. The bundling exists for ergonomics: cleaner names,
easier set-membership, easier instantiation. A `type` can carry
constraints that maintain its wholeness (e.g., `Person` asserting
`age ∈ {0..200}` because Persons with negative age make no sense). A
`type` has no methods, has no identity, is not an object. It's a
named cluster of related variables and the constraints connecting
them.

**`claim` — a predicate / statement / mixin of constraints.**
The content of a claim is constraints: equalities, set-membership,
comparisons, logical connectives, or composed claims (referencing
other claims by merging their constraints). A claim is NOT a function.
"Using" a claim means *merging its constraints* into the surrounding
system. Claims appear inside `type` and `fsm` bodies. A claim can be
thought of as a property — `Person` has a property "reasonable_age"
expressed as a claim about that Person's `age` variable.

**`fsm` — a constraint system with the state-pair convention.**
An `fsm` has variables like a `claim` or `type`, but each variable
gets duplicated: a parameter `x ∈ S` produces `_x` (previous tick) and
`x` (this tick). The body asserts the transition relation linking
them. The runtime ticks the FSM: each tick, `_x` is pinned to the
prior tick's solved `x`, the body solves, the cycle repeats until a
halt condition (state stable AND no effects emitted).

**`fti` — an `fsm` that emits `libcall`.**
Same state-pair semantics as `fsm`, plus the ability to emit
`LibCall` effects bridging to external state machines. Stack, Queue,
files, sockets, GPU buffers, any C-library-backed
resource — all are FTIs. The Z3-side variables model the visible
state; the libcalls keep external state synchronized. **External
auxiliary memory — what bounded-per-tick FSMs need to act as PDAs,
Mealy/Moore machines, parsers — is built as FTIs. FTI is part of v1.**

## The architecture in one paragraph

The runtime is two things: a **trampoline** (`src/runtime.py`, ~130
lines) that runs an SMT-LIB FSM body to halt, and **libcall**
(`src/ffi.py`, ~80 lines) that bridges to any C library via ctypes.
Everything else — Z3 access, data structures, multi-FSM patterns,
JIT — is library code (in `prelude/` and beyond) that uses libcall.
See [`docs/runtime-architecture.md`](docs/runtime-architecture.md)
for the full design rationale.

## Examples that work today

```
claim sum_is_eight()
    x ∈ {0..10}
    y ∈ {0..10}
    z ∈ {0..20}
    z = x + y
    x = 3
    y = 5
```

```
fsm Counter(count ∈ {0..5})
    count = _count + 1
```

## How to run code

```
python3 src/main.py FILE.ev               # run; prints the final model
python3 src/main.py --emit-smt FILE.ev    # just emit the SMT-LIB
```

## What goes in Evident, not Python

- **The prelude.** Z3 set-theoretic bindings as claims/FTIs; the FTI
  declarations for external memory (Stack, Queue, etc.); idiomatic
  set / sequence / map operations.
- **All data structures.** Stack, Queue, Map, Mailbox — FTIs wrapping
  external state. Not in Z3 datatype space; the Z3 model holds a
  bounded view, libcalls handle the rest.
- **All effect-mediated types** (Mutex, Channel, File, Socket) — FTIs.
- **Multi-FSM composition.** Compile-time composition (one combined
  body) for tightly coupled FSMs; supervisor-pattern FSMs for loosely
  coupled ones. Both are written in Evident.

## What stays in Python

- The trampoline loop and state-pair handling (`src/runtime.py`).
- libcall + ctypes marshaling (`src/ffi.py`).
- The bootstrap parser (`src/parser.py`).
- The bootstrap transpiler (`src/transpile.py`).
- The CLI entry (`src/main.py`).

That's it. Five files. ~700 lines.

## Failure modes already burned

- **`x_next` / `state_next`.** Wrong. Previous tick is `_x`, current
  tick is `x`. If you find yourself reaching for `_next` suffix
  conventions, you've reverted to imperative thinking.
- **Treating claims as functions.** Claims are relations. "Calling"
  one means merging its constraints. Do not write claim bodies as if
  the lines are sequential statements; they are simultaneous
  constraints.
- **Treating Stack/Queue as Z3 data structures.** They're not. They
  are external state machines wrapped as FTIs. Push/Pop are libcalls
  to external memory, not Z3 sequence operations.
- **Putting growing data in the FSM body.** Bodies must be bounded
  per tick. Unbounded data lives outside, accessed via FTI libcalls.
- **Re-rendering the body per call.** The body is parsed once; inputs
  are pinned via the state-pair convention. No `.j2` templates.
- **Imperative thinking.** No `if/then/else`, no `let`, no method-call
  syntax, no "do A, then B" sequences. Those are not Evident.
- **Function-call mental models.** `head(s)` is not a function call.
  It's a value expression denoting the head element of s, used in a
  constraint relating s and that head.
- **Trying to add features in Python.** Add them in Evident.

## Why this matters

You are competing against decades of internalized procedural and
functional programming patterns. Every textbook, every tutorial,
every other repo on this machine pushes you toward "compute X then
use X." Evident does not work that way. When in doubt, ask: "What
relation am I asserting? What are all the variables and how are they
related?" Not: "What step happens first?" There are no steps. There
are only relations and the assignment Z3 finds.
