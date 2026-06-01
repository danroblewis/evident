# The Evident Runtime Architecture

Operational architecture decisions captured from design sessions through
2026-06. Future agents should read this alongside `CLAUDE.md` — that file
covers code-style invariants; this file covers the larger architectural
shape we've settled on.

The destination architecture isn't fully built yet. Where current code
differs from the destination, that's noted.

## The minimum runtime is two things

The Evident runtime consists of exactly:

1. **A trampoline.** Re-invoke a constraint model until it halts. Each
   tick: pin variables, solve, read outputs, emit effects between ticks,
   thread state forward. ~10 lines of Python today; expected to stay
   small in Rust.
2. **libcall.** A C FFI bridge with a signature grammar `r(args)` where
   `r` and each arg is one of `i/l/d/s/v` (int/long/double/string/void).
   ~60 lines. The signature grammar is parsed by an SMT-LIB FSM
   (`stdlib/parse_sig.smt2`), so even the FFI dispatch is self-hosted.

That's the runtime. Everything else is library code. Specifically:

- **Z3 is a library**, not a runtime. Accessed via `libcall("libz3", ...)`
  to invoke the Z3 C API directly. The `z3-via-libcall-experiment`
  branch validates this works end-to-end with zero runtime changes.
- **Data structures** (stack, queue, map, mailbox) are libraries built
  on Z3 datatype constructors. Each push allocates a new Z3 node; Z3's
  refcounting handles lifetime.
- **JIT compilation** is a library — `libcall("libtcc", ...)` or
  `libcall("libgccjit", ...)`. Evident programs walk Z3 models, emit C
  or IR, compile, get function pointers, call them.
- **Multi-threading**, when needed, is `libcall("libc", "pthread_*", ...)`.

The runtime never gets a "Z3 module," a "data-structures module," a
"JIT subsystem," or a "scheduler." If you find yourself adding one to
the runtime, stop and ask whether it can live as library code instead.
The answer is usually yes.

## Synchronous semantics within a tick

**Within one tick, all FSMs' constraints hold simultaneously.** Z3
finds an assignment satisfying all of them. There is no notion of "FSM
A runs before FSM B" within a single tick. Cross-FSM dependencies are
expressed as shared variables; Z3 propagates values automatically.

This is the **synchronous-language regime** (Esterel, Lustre, Signal).
The constraints:

- Each FSM's transition relation is applied **exactly once** per tick.
- All FSMs see each other's outputs through shared variables, resolved
  simultaneously by the solver.
- No FSM gets a "second turn" within a tick.

If you need iteration that exceeds one application, you have two
options:

1. **Compile-time unrolling** — write the body with N copies of the
   transition relation, threading intermediate variables between them.
   Use this when the iteration count is bounded and known at compile
   time (fixpoint computations over fixed-size structures, LR closure
   computation, etc.).
2. **Multi-tick execution** — let the FSM run once per tick and
   converge over time. Use this for unbounded iteration or for cases
   where one logical step legitimately takes multiple ticks.

There is **no intra-tick iteration mechanism** and we are not adding
one. The Lisp-shaped machinery for "pause and resume mid-tick"
(continuations, coroutines, algebraic-effect handlers) is explicitly
out of scope. The synchronous regime is sound, compiles efficiently,
and gives us the FPGA/GLSL/JIT story we want.

## Composition over scheduling

Two situations, two patterns:

### Tight coupling — compose at compile time

When multiple FSMs share state and run in lockstep, **compose their
bodies into one combined FSM** at compile time. Variables are
namespaced; shared variables are unified; transition relations are
AND'd together. The trampoline runs ONE FSM whose body is the
composition.

Benefits:
- Z3 solves them together — full simplification and propagation
- Subscriptions are free — Z3 sees all dependencies automatically
- No runtime scheduling overhead
- Native compile target (synchronous languages compile this way)

### Loose coupling — separate runners + mailboxes

When FSMs evolve at different rates, or interact only through explicit
messages, each is **a separate trampoline instance** that communicates
through mailbox FTIs (see below). One name for this pattern: **Actors**
— processes with private state and an inbox.

The supervisor that schedules them is also an Evident program — an
FSM whose tick decides which child actor to advance. The runtime
never knows about actors or supervision; both are library code.

### What we do NOT have

- No runtime multi-FSM scheduler.
- No runtime subscription mechanism.
- No runtime "FSM A then FSM B" sequencing primitive.

If you want a particular execution order, build a composite FSM that
encodes the order in its body. If you want independent execution
rates, spawn separate trampoline instances.

## Z3 expression nodes are the cons cells of Evident

The foundational compound data structure is **the Z3 AST node**. Built
by `Mk_*` libcalls (`Z3_mk_int`, `Z3_mk_const`, `Z3_mk_eq`, etc.),
composed into trees by `Mk_*` operators, stored as integer handles
that index into Z3's handle table.

This is the Lisp move. In Lisp:
- atoms + cons cells generate every tree-shaped structure
- code is data is lists
- the runtime stays tiny; library code is large

In Evident:
- atoms (constants, variables) + expression nodes generate every
  constraint structure
- code is data is constraint models
- the runtime stays tiny (trampoline + libcall); library code is large

**The cell array is library-level, not runtime-foundational.** It was a
useful experiment that proved we could carry PDA-class work, but the
deeper primitive is the Z3 expression node. The cell array's role —
host-owned indexed memory — is satisfied by Z3's handle table once we
go fully Z3-via-libcall.

Current code still uses cell arrays (`EffectsFsmRunner` has them, the
LR parser uses them). That's transitional. The destination is for
auxiliary memory to live as Z3 datatype chains, with the cell array
either removed or repositioned as a peripheral capability for the
streaming-effects (Architecture A) use case.

## FTI — Foreign Type Interface

For ergonomic syntax, types carry behavioral semantics. A Foreign Type
Interface declaration wraps the libcall ceremony for one type:

```
fti Mutex(T)
    handle ∈ Int                    ; pthread_mutex_t* as long

    on_read result ∈ T =
        effects = [lock, load, unlock]

    on_write new ∈ T =
        effects = [lock, store, unlock]
```

User code then uses the type as if it were a normal variable:

```
schema Counter
    count ∈ Mutex(Int)

fsm Counter(count)
    count_next = count + 1
```

The transpiler expands `count` reads to `on_read` effect sequences and
`count_next = ...` writes to `on_write` sequences. Users never write
libcall soup; the soup lives in one place per type.

This unifies several things we previously discussed as separate:

- Effect-bearing variables (Mutex, Atomic) — FTIs.
- Mailboxes, channels, queues — FTIs.
- Foreign C structs (memory layout + field access) — FTIs.
- File handles, network sockets, GPU buffers — FTIs.
- Capability-wrapped variables — FTIs.

One construct. The constraint-style `∈` syntax stays universal:

| Form | Meaning |
|---|---|
| `x ∈ Int` | host sort, no constraint |
| `x ∈ {0..10}` | host sort + range constraint |
| `x ∈ MyEnum` | datatype membership |
| `x ∈ Mutex(Int)` | host sort + FTI behavior |
| `x ∈ InMessage(T)` | host sort + read-as-effect behavior |

Same primitive, six positions.

## Architecture A is a library pattern on Architecture B

Two execution shapes:

**Architecture A — per-tick solve over a static body.**
The body is built once, solved many times with different pins. The
classical FSM tick loop.

**Architecture B — accumulating model, solved once.**
The model grows as the program reads input; solved at the end (or
intermittently). Used for type checking, planning, constraint
generation, model checking.

**A is a library pattern over B's primitives.** You build a model
once, then use SolverPush/SolverPop to layer temporary pins per tick.
This is how incremental SMT works at the DPLL(T) level and is well-
tested. The current FsmRunner's per-tick `s.check(*pins)` pattern is
an instance of this; we just haven't named it as "A on B" before.

Implication: the runtime doesn't need to commit to A or B as the
primary mode. Both are library patterns over the trampoline +
libcall(Z3) primitives. A streaming program (Mario) is A-style. A
type checker is B-style. Same runtime.

## The bootstrap path

The runtime is small enough to build now. The libraries are large
enough that bootstrapping them is the project's real work.

**Order of operations:**

1. Runtime: trampoline + libcall. Already exists in `src/` (with the
   transitional cell-array addition).
2. Hand-write minimal stdlib in SMT-LIB: Z3 bindings, mailbox FTI,
   Mutex FTI, a few data structures. Maybe 500-1000 lines total.
3. Build the Evident parser using the SMT-LIB stdlib. The parser is
   itself an Evident program (B-style: it reads tape, builds a Z3 AST
   that represents the source program's meaning).
4. Once the parser works, rewrite the stdlib in Evident. The SMT-LIB
   versions become reference implementations.
5. Iterate: extend the language surface, add FTIs, build domain
   libraries (math, IO, graphics, network).

The runtime does not grow during steps 2-5. New features become
either library code or FTI declarations.

## What NOT to add to the runtime

This list exists because each of these has been proposed and is wrong:

- **A Z3 wrapper.** Z3 is reached via libcall. Adding a Python-level
  wrapper is unnecessary plumbing.
- **A scheduler / multi-FSM dispatcher.** Compose FSMs at compile time
  for tight coupling; use the supervisor library pattern for loose
  coupling.
- **A subscription mechanism.** Z3 propagates values automatically
  within the combined model.
- **Mailboxes, queues, channels as primitives.** These are FTI library
  code.
- **A JIT subsystem.** Use libcall to libtcc / libgccjit / libLLVM.
- **A garbage collector.** Z3's handle table refcounts; cells (if
  retained) use a bump allocator with no free in v1.
- **A type system in the runtime.** Types are declared in Evident via
  FTI declarations; the runtime never inspects them.
- **An effects-handler subsystem.** FTI on_read / on_write expansion
  is a compile-time transformation.
- **A continuation / coroutine mechanism.** Synchronous semantics
  forbid intra-tick iteration; no machinery needed for it.

## Failure modes we've burned ourselves on

- **Re-rendering the SMT-LIB body per call.** Killed by the `.j2`
  template removal. Body is parsed once; inputs are pinned via the
  state-pair `_input`/`input` channel. Z3 caches across calls.
- **Putting growing data in the FSM body.** The 431-second parser bug.
  Fix: move growing data to a host-owned tape (cells today; Z3 datatype
  chains in the destination).
- **Imperative thinking.** Evident is relational. Programs describe
  WHAT a valid answer satisfies; Z3 finds it. `if/then/else`, `let`,
  method-call syntax are not Evident.
- **Trying to optimize multi-FSM perf by splitting.** For tightly
  coupled small numbers (~20) of FSMs, compose into one combined
  model. Z3's simplification often beats hand-coded scheduling.
- **Treating the cell array as foundational.** It was a useful
  experiment; the deeper primitive is the Z3 expression node.

## How current code maps to this architecture

- `src/fsm_runner.py` — the trampoline. Correct in shape.
- `src/minimal.py` — `EffectsFsmRunner` adds libcall + cell array.
  Cell array is transitional.
- `src/ffi.py` — libcall. Correct.
- `src/stdlib/*.smt2` — early stdlib files (lex, parse, parse_sig,
  toposort). These will be regenerated as Evident programs eventually.
- `examples/lr_parser.smt2` — proves cell-array PDA capability.
  Transitional; reference implementation for the future Evident parser.
- `examples/z3_via_libcall_mk.smt2` — proves Z3-via-libcall works with
  zero runtime changes. Validates the destination architecture.

The gap: we still have the cell-array primitive in the runtime. The
destination is to remove it once Z3 datatype chains can carry the
same work, which requires the bindings library to mature.

## The Lisp lesson, one more time

A tiny runtime plus a rich library equals a large but coherent
language. The runtime stops being a "compiler with batteries
included" and becomes a "substrate over which everything is library
code." We invest in libraries — bindings, FTIs, data structures, the
parser, the transpiler, the JIT, the codegen backends — and the
runtime stays unchanged across all of them.

This is the architecture we're committing to. New features go in
libraries. The runtime stays small. The work moves to where the work
is interesting: writing Evident.
