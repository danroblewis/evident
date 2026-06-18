> Ported from the **prototype-z3-python** branch (the Python-over-Z3 prototype where these were
> developed). Being realized on *this* branch over the real Evident runtime +
> example corpus. Path references to `prototype/...` are historical; the live
> implementation lives in `viz/`.

# Effectful / IO programs as models — open systems, effect traces

> Companion to [`phase-portraits.md`](./phase-portraits.md) and
> [`state-space-diagrams.md`](./state-space-diagrams.md). The model examples so far
> (`sum_to`, `gcd`, `list_sum`, …) are **closed** systems: pure folds whose answer
> ends up in the final state. Real utility programs — `echo`, "read two values and
> add them", a parser — are **open**: they read inputs and write outputs as
> *effects*. This note is how those fit, and why they want different diagrams.
> Demo: `prototype/utility_programs.py`.

## Closed vs open

| | closed (a fold) | open (a utility program) |
|---|---|---|
| example | `sum_to`, `gcd`, `list_max` | `echo`, `add2`, an LR parser |
| inputs | all given up front (the initial state) | arrive over time, from outside (stdin, network, a clock) |
| outputs | the final state | a stream of emitted effects (stdout, writes, FFI) |
| "the answer" | a fixed point you read off | the **effect trace** itself |
| best diagram | phase portrait (state flows to the answer) | control graph + **timing / effect trace** |

A closed model is a function; an open one is a **reactive process**. The phase
portrait of an open program's *internal* state is usually trivial (a tiny program
counter), because the interesting content is on the I/O boundary, not in the state.

## How effects fit the Z3 + harness model

The whole point of the "Z3 + minimal harness" runtime is that effects live in the
*harness*, not the solve. Two roles:

- **An input (read) is an uninterpreted function.** `read()` is a `declare-fun`
  with no body — the solver knows a value *exists* and is consistent with the
  constraints, but not what it is. The **harness supplies it** when it dispatches
  the read effect (from stdin, a socket, etc.). So in `add2`, `a` and `b` are
  uninterpreted until read; the *sum* `a + b` is a pure constraint over them.
- **An output (write) is an emitted effect.** The transition that does
  `Write(a+b)` produces an effect token; the harness performs the actual write.

This is the **solve → dispatch loop**: each tick, the solver computes the next
state *and* which effects to emit; the harness dispatches them (reads feed back in
as the next inputs, writes go out), then the loop repeats. The computation is
declarative; only the boundary is imperative. (Prototyped in `prototype/effects/`
— `EffectProp` over a Z3 propagator, with real `log` / `echo` / `libcall`
effects.) An effect that crosses the commit boundary must be safely undoable if the
solver backtracks — see the reversibility discussion in the effects notes.

## The three demo programs (`utility_programs.py`)

- **`add2` — "read two values, add them."** Control graph `read a → read b →
  add+write → done`, with the **effects on the edges** (`⚡Read→a`, `⚡Read→b`,
  `⚡Write(a+b)`). The picture makes the split explicit: the value `a+b` is a pure
  constraint; the reads/writes are the only effects. This is the smallest complete
  "utility program."
- **`echo` — "read a line, write it, repeat."** A `read ⇄ write` loop. Its natural
  view is a **timing diagram / effect trace**: stdin reads alternating with stdout
  writes, each datum flowing in then straight back out, until EOF. The loop *is*
  the reactive daemon structure.
- **A toy LR parser** for `E → E + n | n`. Two views:
  - the **parsing automaton as a state-transition graph** (the LR(0) item-set DFA:
    shift/goto edges, reduce/accept states) — the swap-style view of the parser's
    *control*;
  - a **parse of `n + n + n` as a stack-depth-over-step trace** — the shift/reduce
    sawtooth (shifts push, reduces pop).

  An LR parser matters here because it is a **pushdown automaton = a transition
  system with a stack.** It's the first example whose state is *unbounded* (the
  stack), which is exactly the hard case from the recursion taxonomy
  (`docs/notes/recursion-in-z3.md`): bounded state lowers to a cheap solve, but a
  parser needs genuine stack/recursion machinery. So "what would an LR parser look
  like?" answers itself — it's a state-transition graph plus a stack — but it also
  flags the boundary where the bounded-solve story needs the `Done`-loop / explicit
  stack, not a single solve.

## Diagram cheat-sheet for open programs

- **Control / state-transition graph** — the program's skeleton (program counter,
  parser states). Finite control only.
- **Timing diagram / effect trace** — the reads and writes over time; *the* view
  for an IO program (echo, a protocol, a driver).
- **Event/Gantt trace** — effects with durations and the commit boundary.
- (A phase portrait of the *data* is still useful when the computation over the
  inputs is rich — but for glue/IO programs the boundary trace is the point.)

The throughline: a closed model *is* its state space (draw the flow); an open
model *is* its interaction with the world (draw the trace). Same transition-system
substrate; different projection, because the interesting axis moved from *state* to
*effects-over-time*.
