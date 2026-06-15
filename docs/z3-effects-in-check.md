# Getting the loop inside Z3 — effects during `check()`

We have prototyped enough incremental tick loops (the runtime driving Z3 from the
outside). This document is about the other direction: **running the program's
loop *inside* a single `check()`**, with effects dispatched during the solve, so
the host code is a thin propagator rather than a `while` loop. It explains the
mechanism thoroughly, shows a worked example in all three forms (Python /
prettifier / smt2), and is honest about what runs in this environment and what
needs a matched Z3 build.

## The one principle: speculation vs commitment

Z3's `check()` is a **speculative** search — it guesses, propagates, hits a
conflict, backtracks, guesses again. Effects (read, write, libcall) are
**irreversible** — you cannot un-read a line or un-call libc. So an effect may
only fire at a point the solver will *not* take back: a **commit point**.

That single fact organizes everything. The "tick" is not an arbitrary execution
model — it is the commit boundary made explicit. You cannot delete it (effects
are irreversible, search speculates); you can only choose **where it lives** and
**how fine-grained it is**. Every option below is a different placement of the
same boundary.

| placement of the commit boundary | mechanism | who drives the loop |
|---|---|---|
| end of a whole solve | the tick loop | host `while` |
| end of a whole solve, solver reused | incremental `push`/`pop` | host `while` (efficient) |
| each `final` check, inside one `check()` | `UserPropagateBase` | **Z3** |
| each forced decision, inside one `check()` | `UserPropagateBase` (`fixed`) | Z3 (fine-grained, risky) |
| (observe-only) each improving model | `Optimize` + `set_on_model` | Z3 (one-directional) |

Incremental `push`/`pop` is *not* a different boundary from the tick loop — it is
the same boundary made cheap (reuse the solver, keep learned clauses). The genuine
"loop inside Z3" is the propagator: it relocates the commit boundary from the host
`while` into the solver's `final` callback, so a single `check()` runs the program.

## Effects split by direction

The commit boundary is strict for some effects and loose for others:

- **Output / observe** (write, log, emit telemetry) *tolerate* speculation — if
  the solver backtracks over a log line you logged something it abandoned, which
  is usually fine or even what you wanted (you were observing the search). These
  can fire from almost any callback.
- **Input / interactive** (read, then *use* the value) *cannot* tolerate
  speculation — the value has to be both real and permanent, so it must come at a
  commit point.

So "how do we get effects out during the solve" has two answers depending on the
effect: observe effects leak out almost anywhere; interactive effects need a
commit point (`final`, or a level-0 forced assignment).

## The mechanism: `UserPropagateBase`

A *propagator* is host code Z3 calls during the search. You subclass
`z3.UserPropagateBase`, pass it the solver, override the scope hooks, and register
callbacks:

- `push()` / `pop(n)` — Z3 calls these as it enters/leaves decision scopes; you
  mirror your own trail so you know what is speculative.
- `add(term)` — register a term so you get a `fixed` callback when the solver
  assigns it (works for Bool and BitVec terms; arithmetic terms only reach
  `final`).
- `add_fixed(cb)` — `cb` fires when a registered term is fixed to a value.
- `add_final(cb)` — `cb` fires when the solver has a **complete candidate model**
  (a commit point for the current state).
- `add_eq(cb)` / `add_diseq(cb)` — fire when terms become (dis)equal.
- inside a callback you may `propagate(conclusion, justification_ids)` to inject a
  consequence, or `conflict(ids)` to force a backtrack.

The shape of an effect runtime: register the program's **effect-trigger** Bools
(`need_read[t]`), and when one is fixed true, perform the real IO and `propagate`
the world's answer back into the search (`world(t) == <the line read>`). For
safety, do irreversible effects only in `add_final` (a confirmed model) or on
level-0 forced assignments — never on a speculative `fixed`, because the solver
may backtrack and you cannot un-read.

The propagator-driven tick loop, then, is: at `add_final`, dispatch this state's
effects, `propagate` the next state's constraints, and let the same `check()`
continue — Z3 runs the iteration instead of your host code.

## A worked example, in three forms

A 3-step program that accumulates values the world provides. `world(t)` is the
**effect** — an uninterpreted function (no body): the value read from outside at
step `t`. `acc[t]` is pure computation. The whole bounded program is one
constraint system, so it is one `check()`.

### Python

```python
import z3

world = z3.Function("world", z3.IntSort(), z3.IntSort())   # the effect (a read)
acc = [z3.Int(f"acc{t}") for t in range(3)]

s = z3.Solver()
s.add(acc[0] == world(0))
s.add(acc[1] == acc[0] + world(1))
s.add(acc[2] == acc[1] + world(2))
s.check()
```

### Prettifier (our faithful render)

```
acc0 = world(0)
acc1 = acc0 + world(1)
acc2 = acc1 + world(2)
```

### SMT-LIB (what Z3 holds)

```
(declare-fun world (Int) Int)
(declare-fun acc0 () Int)
(declare-fun acc1 () Int)
(declare-fun acc2 () Int)
(assert (= acc0 (world 0)))
(assert (= acc1 (+ acc0 (world 1))))
(assert (= acc2 (+ acc1 (world 2))))
```

Run as-is, the solver picks the under-determined `world(t)` itself (it returns
`0,0,0`). The effect symbol is the body-less `declare-fun world` — exactly "an
uninterpreted function is a description of behavior the model doesn't compute,
i.e. an Effect." To make the *world* supply those values **during** the solve
rather than letting the solver invent them, you attach a propagator.

### The propagator that supplies the effect (host code)

This is the driver that turns the body-less `world` into real reads during the
single `check()`. It is written against z3py's documented `UserPropagateBase`
API. (See the environment note below — it does not execute on this repo's exact
z3 build.)

```python
import z3

class EffectRuntime(z3.UserPropagateBase):
    def __init__(self, s, world, n_steps, world_inputs):
        super().__init__(s)
        self.world, self.inputs = world, list(world_inputs)
        self.lim = []
        self.add_final(self._on_final)          # commit point: a complete model
        # register a Bool trigger per step so `fixed` fires when a read is needed
        self.triggers = [z3.Bool(f"need_read{t}") for t in range(n_steps)]
        for b in self.triggers:
            s.add(b)                             # the program asks to read each step
            self.add(b)
        self.add_fixed(self._on_fixed)

    def push(self): self.lim.append(len(self.inputs_done))
    def pop(self, n):
        for _ in range(n): self.lim.pop()
    def fresh(self, ctx): return EffectRuntime(None, self.world, 0, [])

    def _on_fixed(self, trigger, value):
        # in a MATCHED z3, `trigger` is the Bool expr and `value` its assignment;
        # when a read trigger commits true, perform the real read and propagate it
        t = self.triggers.index(trigger)
        line = self.inputs.pop(0) if self.inputs else 0     # the real-world read
        self.propagate(self.world(t) == line, [trigger])    # force the search to use it

    def _on_final(self):
        pass                                    # confirmed model: nothing left to do
```

The idea: the program *requests* a read each step (`need_read[t]`); when the
solver commits that request, the propagator does the real IO and `propagate`s the
answer, so `world(t)` takes the world's value instead of a solver guess — all
inside one `check()`, no host loop.

## Environment status (honest)

The propagator code above is correct against z3py's documented API, but **it does
not run on this repository's Z3 build**, and the reason is a version mismatch, not
a design flaw:

- This environment has the **z3py wrapper 4.8.12** over **libz3 4.15.4** (system
  `python3-z3`). The user-propagator callback ABI changed between those versions.
- Measured: `add(term)` returns one id (e.g. `771748896`) while the `fixed`
  callback receives a *different*, non-corresponding id (e.g. `773464000`), and
  `fixed` is handed a raw `int` rather than the expression. So the id↔term mapping
  the propagator needs is broken here, and `propagate` cannot be aimed correctly.
- A matched install (pip `z3-solver` ≥ ~4.12, where wrapper and library agree)
  fixes this. That install is not available in this sandbox (no `venv`/`ensurepip`,
  and the source wheel does not build), so we cannot demonstrate it running here.

What *does* run here: the example **model** in all three forms above, and the
whole bounded program in one `check()` with the solver choosing the
under-determined effect values. The mid-solve dispatch is the part that needs the
matched build.

## Backtracking — the real cost of going inside

The propagator buys you "one `check()` runs the program," but you take on the
speculation problem in full:

- **Irreversibility.** Effects fired on a speculative `fixed` can be undone by the
  solver but not by the world. Gate interactive effects to `add_final` or
  level-0; buffer-and-commit the rest.
- **Ordering.** The solver fires `final`/`fixed` in *its* search order, not your
  program order. Per the under-determination design, that is sometimes exactly
  right (any valid interleaving) and sometimes a bug you must constrain away
  (a write that must precede another needs an explicit happens-before constraint).
- **Re-entrancy.** Inside a callback you are mid-solve; you can `propagate` and
  `conflict` but you cannot freely re-assert or re-`check`.

This is why the incremental tick loop remains the safe default — its commit
boundary (a finished solve) is unconditionally safe. The propagator is the
power-tool: it makes `check()` run the loop, at the price of owning
backtracking-safety and losing easy control of effect order.

## Where this leaves the plan

The honest result: "the loop inside Z3" is real and the mechanism is
`UserPropagateBase` at `final`, but it is gated behind two things —
a matched Z3 build (this sandbox's is mismatched) and a real solution to the
backtracking/ordering problem. The next concrete step, on a matched Z3, is the
minimal probe: a propagator that at `add_final` performs one fake read,
`propagate`s the value, and lets a 2–3 step program complete inside a single
`check()`. If it feels like Z3 running our program, we adopt it; if it is a
backtracking minefield, we keep the incremental tick loop and use the propagator
only for observe-effects. Either answer is worth having before committing to a
runtime shape.

See `z3-beyond-smt2.md` ("Getting effects out of a solve") for how this sits in
the larger map of Z3 capabilities.
