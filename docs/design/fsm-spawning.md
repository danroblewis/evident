# User-FSM Spawning

Status: design (no implementation, no immediate plan).

## What's missing today

Today, the set of user-defined FSMs in an Evident program is
**static**. At load time, the runtime walks every top-level claim
that matches the FSM shape (state pair + EffectList + ResultList +
optional world) and turns each into one FSM instance. That set
doesn't change for the program's lifetime.

What we don't have: a way to instantiate an FSM **dynamically** —
for each new TCP connection spawn a connection-handler FSM, for
each REPL submission spawn an evaluator FSM, for each work item
in a queue spawn a worker FSM.

## Why it matters

The static model handles a lot of programs well:

  * Single-FSM scripts (the bulk of small evident demos).
  * Fixed-N coordinator programs (game state + render + setup,
    producer + consumer + supervisor).
  * Plugin-augmented programs (echo, counter loops).

It runs out of expressiveness when N depends on runtime data:

  * **Network server**: 1 listener FSM + N connection FSMs (N grows
    with `accept`). Today you'd write one big FSM that branches over
    a `Seq` of connection states — workable for a few connections,
    awkward for many, doesn't get independent scheduling.
  * **REPL with parallel evaluation**: 1 input FSM + N evaluator
    FSMs. Each user submission spawns an evaluator; evaluators
    finish independently.
  * **Worker pool**: 1 dispatcher + N workers. Workers come and go
    as the work changes.
  * **Per-resource state machines**: e.g. one FSM per open file,
    each managing the file's read/write state. The FTI design is
    related but covers Rust-side bridges, not user-side per-instance
    FSMs.

In each case, the alternative ("one FSM that loops over a Seq of
sub-states") gives up the architectural wins of multi-FSM:
independent scheduling, per-FSM solve cost, separate halt
lifecycle, separate effect dispatch.

## Design space

Three intertwined questions:

### 1. How is a "template" declared?

Probably as a regular `claim`, with an annotation marking it as
spawnable. Something like:

```evident
spawnable claim connection_handler(conn ∈ TcpConnection,
                                   state, state_next ∈ HState,
                                   ...)
    -- per-connection logic
```

`spawnable` would mean: the runtime doesn't auto-instantiate this
at load. Some other FSM (or plugin) calls `spawn` to create
instances.

Alternatively, every claim is implicitly spawnable, and the
distinction is just whether anything ever spawns it. Whichever
side you pick, the runtime needs a way to know "create a new
instance" at runtime.

### 2. What's the spawn primitive?

Three shapes possible:

  * **Effect-based**: `Effect::Spawn("connection_handler", initial_world_values)`.
    The dispatcher creates a new FSM instance, registers it with
    the scheduler. Result: an opaque instance ID.
  * **Declarative**: world has a `Set` or `Seq` of "instance
    descriptors" — adding to the set spawns; removing kills.
  * **Plugin-mediated**: plugins (like a TCP listener) spawn
    on-demand based on external events; users don't spawn directly.

Effect-based is most explicit. Declarative fits the "everything
through shared state" framing best. Plugin-mediated covers the
common use cases without exposing a general primitive.

Probably effect-based for v1; declarative as a sugar layer on top.

### 3. How do parent / children / siblings communicate?

Parent → child: world fields scoped to the child instance. The
parent writes a "request" field; the child wakes via delta and
processes it. The scheduler routes the parent's writes to that
specific instance.

Child → parent: write a "response" field on the parent's world.
Parent reads via delta.

Siblings: communicate via the parent or via a shared world the
parent owns.

The trickiness: world fields today are global to the program. With
N instances of the same template, each instance needs its own
private world view AND a shared world for cross-instance
coordination. Probably:

  * Each spawned instance gets a fresh "instance world" (private).
  * The parent's world is read-only to children.
  * Children can write a designated "response" channel.

This is essentially the `actor` pattern — each instance has
private state + a mailbox.

## What identity / lifecycle looks like

  * **Spawn**: parent calls `Spawn(template, init_args)` → gets an
    instance ID. Runtime creates instance, schedules it.
  * **Address**: parent writes `world.children[id].request_field = v`
    to send work. Child reads via subscription.
  * **Halt**: instance halts via the same mechanisms as static
    FSMs (no inputs left, or `Effect::Exit` from itself). Parent
    can also explicitly kill via `Effect::Kill(id)`.
  * **Garbage collect**: when an instance is halted AND no parent
    references it, runtime drops it. Frees the world fields.

## What this overlaps with

  * **FTI** (`foreign-type-interface.md`): bridge plugins
    materialize C-side resources. Per-resource state machines.
    Same identity question (multiple declarations = N resources or
    1?). The mechanisms differ — FTI is Rust-side, FSM-spawning is
    evident-side — but the conceptual gap is the same.
  * **Multi-writer scheduling**: each instance is a separate
    writer of its own private world. Per-instance write-sets are
    auto-disjoint (different instance IDs).
  * **Schema interface** (`schema-interface.md`): each instance is
    a schema. The unified read-set/write-set/state/schedule/behavior
    model applies. Spawning just creates a new bag of those.

## Why we're not building it now

  * **No concrete user demand yet.** Existing demos work without
    it. Most planned next-steps (game loops, declarative scenes,
    GLSL transpilation) don't need dynamic instances.
  * **Big design space.** Each of the three questions above has
    multiple defensible answers. Without a real use case driving,
    the design would be speculative.
  * **Cheap to add later.** The runtime's scheduler already
    iterates a `Vec<MainShape>`. Spawning is just `Vec::push` plus
    setup of per-instance state. The hard part is the syntax /
    semantics, not the runtime plumbing.

When a real use case shows up (probably a TCP server demo), this
doc becomes the starting point for a concrete design.

## See also

  * [`schema-interface.md`](schema-interface.md) — what an
    instance IS (a schema with read-set/write-set/state/etc.).
  * [`foreign-type-interface.md`](foreign-type-interface.md) —
    related "instances of declared things" question, but for
    Rust-side resources.
  * [`multi-fsm.md`](multi-fsm.md) — current static-multi-FSM
    composition pattern.
