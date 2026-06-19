# State Machines as Relations

This document is the conceptual anchor for Evident's FSM model.
Read it before making design decisions about `fsm`, `external`,
or the run loop.

## The core claim

**An Evident program run via `effect-run` is a finite state machine
operating over global state.** The FSM:

1. Reads variables from the global state (the previous tick).
2. Writes new values to those variables (this tick).
3. The runtime ticks it and persists the writes.

A `claim` denotes a relation; an `fsm` is that relation given a
time dimension — the runtime solves it once per tick. The FFI
bridge (typed resource materialization) is Rust code the runtime
runs around each tick because it needs OS access; it is not a
second FSM.

## Claims are relations

A `claim` denotes a **set of tuples**: the set of all
parameter-value assignments that satisfy the claim's body.

```evident
claim add_one(x ∈ Int, y ∈ Int)
    y = x + 1
```

The meaning of `add_one` is the set:

```
{ (x, y) ∈ Int × Int : y = x + 1 }
```

This is standard relational algebra. The claim's body is a
predicate; the claim's denotation is the set of tuples satisfying
that predicate.

### Three equivalent ways to invoke a claim

All three forms produce the same conjunction of constraints:

```evident
-- Positional invocation
add_one(my_x, my_y)

-- Mapsto invocation (explicit binding direction)
add_one (x ↦ my_x, y ↦ my_y)

-- Tuple-in-relation membership (most relational)
(my_x, my_y) ∈ add_one
```

The third form makes the relational reading visible: a claim is a
set, `∈` is set membership, and invocation is asserting a
particular tuple is in that set.

### Why `Set`, not `Seq`

The collection of satisfying tuples is unordered: Z3 may return
any satisfying assignment when queried. There's no "first
satisfying tuple." Within each tuple, positions are ordered (slot 0
= first parameter, slot 1 = second, ...) per the claim's
declaration. So: tuples are ordered internally; the set of tuples
is unordered.

## FSMs are claims with a time dimension

An `fsm` is a `claim` that gets **scheduled**: the runtime
invokes it once per tick. Inside the fsm's body, two views of
each shared variable are available:

- `count` — the value being computed for this tick (post-tick).
- `_count` — the value at the start of this tick (the previous
  tick's `count`).

That's the entire difference between `fsm` and `claim`. The fsm
ticks, the runtime persists the post-tick values, the next tick
reads them as pre-tick values.

```evident
fsm counter
    count ∈ Int
    count = _count = ⊥ ? 0 : _count + 1
```

The runtime:
1. On tick 0, `_count = ⊥` (no previous value); `count = 0`.
2. On tick 1, `_count = 0`; `count = 1`.
3. On tick 2, `_count = 1`; `count = 2`.

The global state's `count` slot accumulates the values monotonically.

## State carries across ticks by name

A variable in the FSM body is a slot in the global state. Its
value persists across ticks: `_count` is the previous tick's
`count`, and the `count` the FSM writes this tick becomes next
tick's `_count`. Name identity is how a value flows from one tick
to the next — no special syntax.

```evident
fsm counter
    count ∈ Int
    count = _count = ⊥ ? 0 : _count + 1   -- accumulate across ticks
```

## The `external` modifier

A schema is `external` when it crosses the OS boundary. Two
combinations:

- **`external type T(...)`** — a typed OS resource. The runtime
  owns its lifecycle (allocate, write-fields, free) via the
  declarative-install bridge. Example:
  `external type SDL_Window(title ∈ String, handle ∈ Int)`.

- **`external claim foo(...) ... = LibCall(...)`** — an
  effect-building helper. Constructs `FFICall`/`LibCall`/`FFIOpen`/
  `FFILookup` values that the runtime's effect dispatch
  executes. Only `external` schemas may construct these effect
  variants — enforced at load time.

## What the relational model eliminates

Compared to earlier versions of `fsm`, the relational model
eliminates several special cases:

1. **No implicit parameter injection.** An fsm does not
   automatically have `state_next`, `last_results`, or `effects`.
   It declares only the variables it actually uses.

2. **No "input" vs "output" variable types.** Everything is just
   a slot. `Seq(Effect)` is special only in that the runtime's
   effect dispatch reads from any slot of that type.

3. **No state-enum requirement.** A counter `count ∈ Int` is a
   perfectly valid fsm. Discrete modes (`mode ∈ {Idle, Active}`)
   are useful when behavior branches on them, not required.

## Reading guide

The picture in one sentence: **A program is an FSM ticking over
global state, where each variable is a slot whose value carries
from one tick to the next.**

When reading an FSM program:

1. Look at the `fsm` declaration and note its variables. Each
   variable is a slot in the global state.
2. For each `_var` reference, the slot's previous-tick value
   feeds in.
3. For each variable assignment in the fsm body, the slot gets
   the new value at end-of-tick.

The graph of "what reads what, what writes what" across ticks is
the program's data-flow shape; the run loop turns it into a
tick-by-tick execution.

## Decision criteria for future syntax

If a new construct would express something orthogonal to "an fsm
ticking over global state," it does not belong in `fsm`'s syntax.
If it can be expressed as a constraint over the same state, it
does. The `fsm` keyword should remain a thin layer: a run-loop
hint and the `_var` time-shift convention. Everything else is
state over time.

A specific consequence:

- **Hierarchical states (Statecharts)**: expressible as nested
  enum-typed variables. No new syntax needed; existing record
  types and enums compose.

If this becomes painful in practice, revisit. Until then, the
language stays smaller.
