# State Machines as Relations

This document is the conceptual anchor for Evident's FSM model.
Read it before making design decisions about `fsm`, `external`,
state sharing, or the scheduler.

## The core claim

**Evident is a coordination language for finite state machines
operating over shared global state.** Every FSM in a program —
whether written in Evident or implemented in Rust as a runtime
bridge — participates in the same coordination model:

1. Read variables from the shared state.
2. Write new values to those variables.
3. The runtime schedules turns and persists writes.

Nothing in the language singles out one FSM as special. The
runtime's effect-dispatcher, stdin-reader, frame-timer, and FFI
marshaller are FSMs that happen to be implemented in Rust because
they need OS access. They share state with user FSMs the same way
two user FSMs share state.

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

## Coordination is by name

When two FSMs both declare a variable with the same name, they
share that slot in the global state. No special syntax — name
identity IS the coordination mechanism.

```evident
fsm producer
    count ∈ Int
    count = _count = ⊥ ? 0 : _count + 1

fsm consumer
    count ∈ Int        -- same `count` as producer's
    last_seen ∈ Int
    last_seen = count  -- read the producer's write
```

The runtime schedules writer-first per turn: `producer` runs,
writes `count`; then `consumer` runs, reads the freshly-written
`count` and writes `last_seen`.

This generalizes to N FSMs over M shared slots. The scheduler's
job is to figure out a valid order (no writer writes after a
reader has already read this turn) and tick the FSMs.

## The `external` modifier

A schema is `external` when it crosses the OS boundary. Three
combinations:

- **`external type T(...)`** — a typed OS resource. The runtime
  owns its lifecycle (allocate, write-fields, free). Example:
  `external type SDL_Window(title ∈ String, handle ∈ Int)`.

- **`external claim foo(...) ... = LibCall(...)`** — an
  effect-building helper. Constructs `FFICall`/`LibCall`/`FFIOpen`/
  `FFILookup` values that the runtime's effect-dispatcher
  executes. Only `external` schemas may construct these effect
  variants — enforced at load time.

- **`external fsm name`** — a runtime-side bridge FSM. Its body
  is implemented in Rust because it needs OS access; its
  declaration in Evident is the **contract** for which shared-state
  slots it reads and writes. Examples:

  ```evident
  external fsm stdin_reader
      stdin_line ∈ String   -- runtime writes
      stdin_seq  ∈ Int      -- runtime writes

  external fsm frame_timer
      tick_count ∈ Int      -- runtime writes; rate from EVIDENT_TICK_MS

  external fsm effect_dispatcher
      effects      ∈ Seq(Effect)   -- runtime reads, then clears
      last_results ∈ Seq(Result)   -- runtime writes
  ```

The Rust implementation of each bridge lives in
`runtime/src/event_sources/`. The declaration in
`stdlib/runtime.ev` is what user code reads to know which slots
exist.

## What this redesign eliminates

Compared to earlier versions of `fsm`, the unified relational
model eliminates several special cases:

1. **No implicit parameter injection.** A user fsm does not
   automatically have `state_next`, `last_results`, or `effects`.
   It declares only the variables it actually uses.

2. **No "input" vs "output" variable types.** Everything is just
   a slot. `Seq(Effect)` is special only in that the runtime's
   effect-dispatcher (a bridge) reads from any slot of that type.

3. **No state-enum requirement.** A counter `count ∈ Int` is a
   perfectly valid fsm. Discrete modes (`mode ∈ {Idle, Active}`)
   are useful when behavior branches on them, not required.

4. **No special `world` keyword.** A multi-FSM program just
   declares shared variables with matching names across FSMs.
   The scheduler infers the writer-reader relationships.

5. **No conceptual gap between user FSMs and runtime FSMs.**
   Both contribute constraints over the shared state. The
   bridges have Rust-side behavior beyond the constraint system
   (OS calls), but their coordination interface is uniform.

## Reading guide

The picture in one sentence: **A program is a collection of FSMs
sharing global state by name; the runtime contributes its own
FSMs for OS-side concerns.**

When reading a multi-FSM program:

1. Look at every `fsm` declaration and note its variables. Each
   variable is a slot in the shared state.
2. Find which FSMs name the same slot. Those FSMs coordinate.
3. For each `_var` reference, the slot's previous-tick value
   feeds in.
4. For each variable assignment in an fsm body, the slot gets
   the new value at end-of-tick.
5. `external fsm` declarations name slots written by Rust-side
   bridges. User FSMs read them by declaring matching names.

The graph of "who reads what, who writes what" is the program's
data-flow shape. The runtime's scheduler is responsible for
turning that graph into a tick-by-tick execution order.

## Decision criteria for future syntax

If a new construct would express something orthogonal to "an fsm
ticking over shared state," it does not belong in `fsm`'s syntax.
If it can be expressed as a constraint over the same shared
state, it does. The `fsm` keyword should remain a thin layer: a
scheduler hint and the `_var` time-shift convention. Everything
else is shared-state coordination.

Two specific consequences:

- **Hierarchical states (Statecharts)**: expressible as nested
  enum-typed variables. No new syntax needed; existing record
  types and enums compose.
- **Parallel regions**: expressible as multiple fsms sharing
  state. No new syntax needed; multi-fsm coordination already
  handles this.

If either becomes painful in practice, revisit. Until then, the
language stays smaller.
