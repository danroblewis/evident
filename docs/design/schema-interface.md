# What is an Evident model?

For a long time we struggled to articulate what a `claim`/`type`/`schema`
in Evident actually IS at the runtime level. We had pieces — "it's a
constraint over variables," "it's a state machine," "it's a thing
the executor solves" — but nothing that captured the whole picture.

This doc is the answer that finally fits.

## A model is a tick unit

An Evident model (claim, type, or schema) is a **tick unit**
with four things:

  1. **A state read** — the previous tick's values it observes.
     Auto-inferred from `_var` / `_world.X` references in its body.
  2. **Private state** — its own state machine (the `state ∈ S`,
     `state_next ∈ S` pair), carried across ticks.
  3. **A behavior** — a body that, given the previous state +
     world, produces this tick's `state` / `world` writes + a set
     of effects. Expressed as constraints solved by Z3.
  4. **Effects** — the IO it emits this tick (println, FFI calls,
     exit). The runtime dispatches them, then carries the writes
     into the next tick.

That's it. An Evident model is fully characterized by these four
things. Nothing else is part of the interface.

## State carries across ticks

The model's behavior runs once per tick. Each variable is a slot in
the global state: `_var` (and `_world.X`) read the previous tick's
value; an assignment in the body writes this tick's value, which the
runtime persists as next tick's `_var`. There is no separate
coordination mechanism — a value flows from one tick to the next by
name.

```
        tick N-1 state                tick N
        ──────────────                ──────────────
        _var, _world.X    ─reads─►     fsm body (Z3-solved)
                                            │
                                            ├─► var, world.X writes
                                            └─► effects ⟨…⟩
                                            │
                          ◄─persists──── runtime dispatches effects,
                                          carries writes to tick N+1
```

Effects are the model's IO: `Println`, FFI calls, `Exit`. The
runtime dispatches the effect list each tick, then loops.

## The runtime's Rust-side bridges

Some behavior the language can't yet express in Evident — FFI
marshaling, typed OS-resource lifecycle (`SDL_Window` and friends),
signal handling — is implemented in Rust. These bridges are not
separate FSMs the program coordinates with; they are runtime code
that runs around the FSM's tick: materializing declared resources,
mirroring their state into the FSM's world fields, and dispatching
the effects the FSM emits.

The line is "the parts we couldn't yet write in Evident" —
primarily things that need primitives the language doesn't have
(FFI, syscalls, OS callbacks).

## FFI as Foreign Type Interface

C function calls are a poor model for stateful resources. The right
model for `SDL_Window` etc. is a type — the user declares the
window they want, a Rust-side bridge makes it real:

```evident
type SDL_Window
    title       ∈ String
    size        ∈ IVec2
    fullscreen  ∈ Bool

claim my_app
    win ∈ SDL_Window
    win.title = "hello"
    win.size  = IVec2(640, 480)
    -- (the rest of the app)
```

A Rust-side bridge observes that `win` exists, calls `SDL_CreateWindow`
to materialize it, and mirrors window state back into `win.size` etc.
when SDL events arrive. The FSM's read on `win.size` includes real
values. Closing the window is the bridge observing that the program
is exiting / the FSM no longer references it.

This generalizes: every C resource (sockets, files, audio devices,
GPU contexts, child processes) becomes a type with a bridge. The
function-call shape is C's preference, not a fundamental requirement.
Foreign Function Interface → Foreign Type Interface.

The existing `Effect::FFICall` stays for direct C function access.
Full Foreign Type Interface is a v2+ direction (see
[`foreign-type-interface.md`](foreign-type-interface.md)); typed
resources already ride the declarative-install bridge.

### Halt is "the FSM has nothing left to do"

The program halts when a tick changes nothing — no slot got a new
value, no effect was emitted — or when the FSM emits `Effect::Exit`.

## What this gives us

  * **One mental model.** The model is a state read + private state
    + behavior + effects, ticked by the run loop. No special cases.
  * **The Rust boundary is opaque.** A bridge written in Rust today
    could be replaced by an Evident claim tomorrow if the language
    gains the needed primitives — the FSM's view of its world
    fields doesn't change.
  * **Composition is just declaration.** Adding behavior means:
    write it into the FSM body. State flows by name across ticks;
    the runtime persists writes and dispatches effects.

## What's NOT yet covered

  * **Multiple cooperating models** — the program is a single FSM.
    Composing independent FSMs over shared state is not part of the
    current runtime.
  * **Cross-program coordination** — communicating with another
    process or machine isn't covered. Would need network bridges.
  * **Real-time guarantees** — the run loop is best-effort; no hard
    deadlines.

## See also

  * [`foreign-type-interface.md`](foreign-type-interface.md) — FFI
    as typed OS-resource, the bridge direction.
  * [`synchronous-reactive-concurrency.md`](synchronous-reactive-concurrency.md)
    — the OG vision: Evident as a synchronous-reactive language
    (Esterel/Lustre family). The schema-interface model implements
    exactly that picture.
