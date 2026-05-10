# What is an Evident model?

For a long time we struggled to articulate what a `claim`/`type`/`schema`
in Evident actually IS at the runtime level. We had pieces — "it's a
constraint over variables," "it's a state machine," "it's a thing
the executor solves" — but nothing that captured the whole picture.

This doc is the answer that finally fits.

## A model is a coordination unit

An Evident model (claim, type, or schema) is a **coordination unit**
with five things:

  1. **A read-set** — a subset of shared state (`world` fields) that
     this model observes. Auto-inferred from references in its body
     (`world.X` → X is in the read-set).
  2. **A write-set** — a subset of shared state this model produces.
     Auto-inferred from `world_next.X` references. Disjoint from
     every other model's write-set in the same program.
  3. **Private state** — its own state machine (the `state ∈ S`,
     `state_next ∈ S` pair), invisible to other models.
  4. **A schedule** — a policy for when this model runs:
       * For evident-implemented models: delta-driven (run when any
         field in the read-set changes, plus self-feedback triggers).
       * For Rust-implemented models (plugins): runs on its own
         (background thread, OS callback, timer).
  5. **A behavior** — a body that, given the current state +
     world, produces a `state_next` + a set of effects. For
     evident models, expressed as constraints solved by Z3. For
     Rust models, expressed as Rust code.

That's it. An Evident model is fully characterized by these five
things. Nothing else is part of the interface.

## Implementation language is opaque

The five-thing interface is the same whether the model is
implemented in evident (constraint claim, solved by Z3) or in Rust
(plugin code, executed natively). Other models cannot tell the
difference and don't need to.

```
       evident model                 Rust model (plugin)
       ─────────────                 ───────────────────
       read-set                      read-set
       write-set                     write-set
       private state (Z3 var)        private state (Rust struct)
       delta schedule                background thread / callback
       Z3-solved body                Rust-coded body
              │                              │
              └──────────┬───────────────────┘
                         ▼
                ┌─────────────────┐
                │  shared world   │ ← single coordination layer
                └─────────────────┘
                         ▲
              ┌──────────┴───────────────────┐
              │                              │
       (other models — same interface, doesn't matter
        which language they're in)
```

Plugin FSMs are NOT a separate concept from user FSMs. They are
the same kind of thing. The Rust runtime is "the FSMs we couldn't
yet write in evident" — primarily things that need primitives the
language doesn't yet have (async I/O, signal handlers, FFI,
syscalls).

## Coordination is exactly one mechanism: shared-state delta

All inter-model communication happens through writes to the shared
world. There is no other primitive. Specifically:

  * **No event channels.** When something happens, you write a
    field; everyone who reads that field gets triggered. The
    `SchedulerEvent` channel we built was a transitional crutch;
    in the unified model it disappears in favor of plugins writing
    world fields.
  * **No commands.** When you want a model to do something, you
    write a field describing what you want; that model's read-set
    includes the field; it wakes; it does the thing; it writes
    back the result. The "command/response" framing is just
    "write request → read response" in two ticks.
  * **No subscriptions API.** Subscriptions are auto-derived from
    the body — every `world.X` is a subscription on X.
  * **No effects (almost).** Effects ARE writes — to stdout, to
    stderr, to FFI. They're modeled as effect-list in the current
    runtime for historical reasons; in the unified model they
    could be world writes too (`world.stdout_line = "hi"` instead
    of `effects = ⟨Println("hi")⟩`). v1 keeps both forms.

## What this means in practice

### Plugins are first-class models

A `StdinPlugin` declares:

  * read-set: `{}` (it doesn't read anything from world)
  * write-set: `{stdin_line}` (it writes each received line)
  * private state: `{ thread_handle, eof_flag, line_buffer }`
  * schedule: blocking thread reads from fd 0
  * behavior: when a line arrives, write it to world.stdin_line

A user `EchoFSM` declares:

  * read-set: `{stdin_line}`
  * write-set: `{stdout_line}`
  * private state: a transition graph (probably trivial)
  * schedule: delta-driven (wakes when stdin_line changes)
  * behavior: `world_next.stdout_line = world.stdin_line`

A `StdoutPlugin` declares:

  * read-set: `{stdout_line}`
  * write-set: `{}` (writes to fd 1, which isn't world)
  * private state: `{ output_buffer }`
  * schedule: delta-driven (woken via internal hook on world write)
  * behavior: when stdout_line changes, write to fd 1

The `echo` program is just these three models composed via shared
world. No special "plugin mode," no "event sources." Just shared
state.

### Single-writer-per-field is the only resource constraint

A file descriptor / socket / hardware register has exactly one
owner — that's true at the OS level. In Evident this is enforced
generically as **single writer per world field**. Multiple writers
to disjoint fields are fine; multiple writers to the same field
are a load-time error.

Stdin's fd is owned by the StdinPlugin because no other model has
`stdin_line` in its write-set. SDL's window handle is owned by the
SDLPlugin similarly. The "single-owner-per-fd" rule we discussed
earlier is just a special case of this single-writer rule.

### FFI as Foreign Type Interface

C function calls are a poor model for stateful resources. The right
model for `SDL_Window` etc. is a type — the user declares the
window they want, a bridge plugin makes it real:

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

A Rust SDLPlugin observes that `win` exists, calls `SDL_CreateWindow`
to materialize it, mirrors window state back into `win.size` etc.
when SDL events arrive. The user FSM's read on `win.size` includes
real values. Closing the window is the plugin observing that no FSM
references it anymore.

This generalizes: every C resource (sockets, files, audio devices,
GPU contexts, child processes) becomes a type with a bridge plugin.
The function-call shape is C's preference, not a fundamental
requirement. Foreign Function Interface → Foreign Type Interface.

v1 keeps the existing `Effect::FFICall` for direct C function
access. The Foreign Type Interface is a v2+ direction; this doc
just plants the flag.

### Halt is "no model has anything to do"

The program halts when the union of all models' read-sets +
plugin-pending-events is empty. Equivalent: no field has a pending
delta, no plugin has a pending event, no model has self-feedback
queued. Same definition as today's "no FSM scheduled = halt", just
phrased uniformly.

## What this gives us

  * **One mental model.** Everything is read-set + write-set +
    private state + schedule + behavior. No special cases for
    plugins, sources, effects, or events.
  * **Implementations are interchangeable.** A plugin written in
    Rust today could be replaced by an evident claim tomorrow if
    the language gains the needed primitives. No other model
    needs to change.
  * **Resource ownership is uniform.** Single-writer-per-field
    captures fd ownership, hardware register ownership, and
    "this user FSM owns this state field" without separate rules.
  * **Composition is just declaration.** Adding a new model
    means: write its body. The runtime infers read-set / write-set
    from the body and slots it into the scheduler.
  * **No coordination machinery to learn.** Users don't think
    about events, channels, commands, subscriptions. They write
    state and read state. The runtime handles wakes.

## What's NOT yet covered

  * **Multiple instances of one model** (FSM spawning) — currently
    each claim is a single instance. Dynamic instantiation
    (one-FSM-per-connection, one-per-evaluation) is a separate
    design question.
  * **Cross-program coordination** — models in one Evident program
    can coordinate; communicating with another process or another
    machine isn't covered. Would need network plugins.
  * **Real-time guarantees** — the scheduler is best-effort;
    no hard deadlines. Plugin background threads run at OS
    priority.

## See also

  * [`fsm-subscriptions.md`](fsm-subscriptions.md) — the scheduler
    that implements delta-driven coordination.
  * [`multi-fsm.md`](multi-fsm.md) — the writer/reader composition
    pattern at the program level.
  * [`models-not-programs.md`](../../models-not-programs.md) — the
    earlier articulation that pointed in this direction.
