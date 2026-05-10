# Writing multi-FSM programs

A practical guide to writing programs that compose multiple FSMs
coordinating through shared world state. Read
[`docs/design/schema-interface.md`](../design/schema-interface.md)
first for the underlying model — this guide is the cookbook.

> **Canonical exemplars in this repo**:
> [`programs/demos/`](../../programs/demos/). Every demo there
> is a multi-FSM program — even the single-FSM ones (those are
> just multi-FSM with one FSM, since the multi-FSM scheduler
> is the only execution path). Each demo ships with inline
> `sat_*` / `unsat_*` static tests and an entry in
> `runtime/tests/demos.rs` for end-to-end coverage.
> When adding to that directory, copy the closest existing
> demo's shape. Worked patterns there:
>   * `test_09_two_fsms` — writer/reader sharing world
>   * `test_10_spawn` — dynamic FSM creation via `SpawnFsm`
>   * `test_11_frameclock` / `test_13_timer` — FTI typed
>     resources
>   * `test_14_stdin` / `test_15_signal` — plugin-as-writer
>     (stdin / signals via reserved World fields)
>
> [`programs/demos/COUNTEREXAMPLES.md`](../../programs/demos/COUNTEREXAMPLES.md)
> lists the runtime gaps each demo had to work around — useful
> reading before you assume a missing piece is your own bug.

## When to use multiple FSMs

You want multiple FSMs when your program has **independent
concerns** that:

  * React to different inputs (one watches stdin, one ticks on a
    timer, one handles signals)
  * Have different lifecycles (setup runs once and halts; render
    runs forever)
  * Have different solve costs (slow setup with a transpiler;
    fast per-frame render)
  * Should run independently of each other (one waiting on stdin
    shouldn't block another doing per-frame work)

You don't need multiple FSMs for a simple script (one FSM with
states is enough) or for purely local logic.

## The basic shape

Each FSM is a top-level claim with this signature:

```evident
claim my_fsm(state, state_next ∈ MyStateEnum,
             last_results ∈ ResultList,
             effects ∈ EffectList)
    state_next = ...
    effects    = ...
```

If the FSM reads or writes shared state, also include world:

```evident
claim my_fsm(world ∈ World,                -- reader
             state, state_next ∈ MyStateEnum,
             last_results ∈ ResultList,
             effects ∈ EffectList)
    -- can read world.X
```

```evident
claim my_fsm(world, world_next ∈ World,    -- writer
             state, state_next ∈ MyStateEnum,
             last_results ∈ ResultList,
             effects ∈ EffectList)
    -- can read world.X AND write world_next.X
```

Each top-level claim matching this shape is detected at load time
and run as an FSM. They run in declaration order; writer FSMs run
first within each tick (so readers see writes within the same tick).

## Runtime-managed plugins

The runtime auto-installs background plugins when the World type
declares specific reserved fields. You don't write any "install
plugin" call — declaring the field is the opt-in.

| World field            | Type    | Plugin                                                  |
|---|---|---|
| `tick_count`           | Int     | FrameTimer (rate via `EVIDENT_TICK_MS=<u64>`, default 100ms) |
| `signal_received`      | Int     | SIGINT handler (Ctrl-C)                                 |
| `stdin_line`           | String  | Stdin line reader                                       |
| `stdin_seq`            | Int     | Stdin sequence counter (only useful with stdin_line)    |

The plugin owns its fields — your FSMs can read them but not
write them (load-time error if you try). Multi-writer disjoint
rule: each field has at most one writer.

## Pattern 1: setup-then-render

The canonical "do work once, then do work per frame" shape.

```evident
type World
    -- handles populated by setup, read by render
    window   ∈ Int
    program  ∈ Int
    ...

enum SetupState =
    Init
    Done

claim setup(world, world_next ∈ World, ...)
    state_next = match state
        Init ⇒ Done
        Done ⇒ Done
    world_next.window = match state
        Init ⇒ <created handle>
        Done ⇒ world.window     -- passthrough
    ...
    effects = match state
        Init ⇒ ⟨init effects...⟩
        Done ⇒ ⟨⟩

enum RenderState = Frame

claim render(world ∈ World, ...)
    state_next = Frame
    effects = ⟨per-frame draw effects⟩
```

Setup runs ~2 ticks then halts (its state stops changing AND it
emits no effects). Render runs forever via effect-feedback. The
scheduler drops setup from the per-tick loop after it halts.

Real example: `programs/demos/effect_multi_fsm_transpiled.ev`.

## Pattern 2: stdin echo / line reader

```evident
type World
    stdin_line ∈ String   -- runtime auto-installs StdinSource
    stdin_seq  ∈ Int      -- (optional) sequence counter for gating

enum EchoState = Echoing

claim echo(world ∈ World, ...)
    state_next = Echoing
    -- gate on "is this a new line?" — without it, effect-feedback
    -- would re-emit the current line forever
    effects = (world.stdin_seq > <last_seen_seq>
               ? ⟨Println(world.stdin_line)⟩
               : ⟨⟩)
```

The "last_seen_seq" tracking can live in:
  * **Private state**: `enum EchoState = Echoing(Int)` payload (now
    works since the Z3 panic was fixed)
  * **Another world field**: declare `last_echoed_seq ∈ Int` in
    World, make echo a writer of just that field

Real examples: `programs/lang_tests/multi_fsm/06_echo.ev` (world
field), `08_word_counter.ev` (payload state).

## Pattern 3: graceful shutdown via Effect::Exit

Any FSM emits `Effect::Exit(code)`. The dispatcher defers — all
co-scheduled FSMs in the same tick complete their effects first,
then the runtime halts cleanly with the requested code.

```evident
claim cleanup_fsm(world ∈ World, ...)
    -- watches some condition
    effects = (world.should_quit
               ? ⟨Println("cleaning up..."), Exit(0)⟩
               : ⟨⟩)
```

If you need SIGINT-triggered cleanup, declare `signal_received: Int`
in World. The runtime installs a SIGINT handler that increments
the field; an FSM reading the field is woken on Ctrl-C.

Real example: `programs/lang_tests/multi_fsm/05_graceful_shutdown.ev`.

## Pattern 4: timer-driven counter

```evident
type World
    tick_count     ∈ Int     -- runtime-managed by FrameTimer
    last_seen_tick ∈ Int     -- counter writes this

enum CState = Counting

claim counter(world, world_next ∈ World, ...)
    state_next = Counting
    is_new = (world.tick_count > world.last_seen_tick)
    world_next.last_seen_tick = (is_new ? world.tick_count : world.last_seen_tick)
    effects = (is_new ? ⟨Println("tick")⟩ : ⟨⟩)
```

Run with `EVIDENT_TICK_MS=100 evident effect-run …`.

Real example: `programs/lang_tests/multi_fsm/07_timer_demo.ev`.

## Pattern 5: multiple plugins coordinating

You can declare ANY combination of plugin-managed fields. The
runtime installs each plugin independently; one user FSM can
subscribe to several sources.

```evident
type World
    tick_count  ∈ Int      -- timer
    stdin_line  ∈ String   -- stdin
    stdin_seq   ∈ Int      -- stdin counter
    last_t, last_s ∈ Int   -- watcher's progress
    events_seen ∈ Int

claim watcher(world, world_next ∈ World, ...)
    new_t = (world.tick_count > world.last_t)
    new_s = (world.stdin_seq  > world.last_s)
    -- update watcher's progress
    world_next.last_t = (new_t ? world.tick_count : world.last_t)
    ...
    effects = (new_t ? ⟨Println("tick")⟩
              : new_s ? ⟨Println("got: " ++ world.stdin_line)⟩
              : ⟨⟩)
```

Real example: `programs/lang_tests/multi_fsm/09_timer_and_stdin.ev`.

## Pattern 6: request/response between user FSMs

Two FSMs coordinating without a special protocol — just disjoint
world fields and standard delta scheduling.

```evident
type World
    request_n     ∈ Int   -- client writes
    request_seq   ∈ Int   -- client writes (bump = "new request")
    response      ∈ Int   -- server writes
    response_seq  ∈ Int   -- server writes (matches request_seq when done)

claim client(world, world_next ∈ World, ...)
    -- gate on "has server caught up?"
    server_caught_up = (world.response_seq ≥ world.request_seq)
    -- only bump request when server's done with the previous one
    world_next.request_seq = (server_caught_up
                              ? world.request_seq + 1
                              : world.request_seq)

claim server(world, world_next ∈ World, ...)
    is_new_req = (world.request_seq > world.response_seq)
    world_next.response = (is_new_req
                           ? <compute>(world.request_n)
                           : world.response)
    world_next.response_seq = (is_new_req
                               ? world.request_seq
                               : world.response_seq)
```

Two writers, disjoint fields. Each side gates on a sequence
counter to know when the other has produced fresh state.

Real example: `programs/lang_tests/multi_fsm/11_request_response.ev`.

## Common gotchas

### Effect-feedback loops

If your FSM emits effects on every wake without gating, it'll
re-schedule itself forever via effect-feedback. Always gate on
"is this actually new work?":

```evident
-- BAD: emits forever after first wake
effects = ⟨Println(world.stdin_line)⟩

-- GOOD: emits only on actually-new lines
effects = (world.stdin_seq > last_seen ? ⟨Println(...)⟩ : ⟨⟩)
```

### Single-writer per field

A field can be written by exactly one schema. If your FSM tries to
write a plugin-owned field (e.g. `world_next.tick_count = ...`),
the runtime errors at load. Pick a different field name.

### ReadLine + StdinSource conflict

A program declaring `stdin_line ∈ String` (auto-installs
StdinSource) cannot also use `Effect::ReadLine` — they'd race for
fd 0. Pick one pattern per program.

### Declaration order matters for writers

Multiple writers run in declaration order. A reader sees all
writers' updates within the same tick; the last writer's value
wins for any given field (though writes are disjoint by rule).

### Initial state for payload state

If your state enum's first variant has a payload (e.g.
`Counting(Int)`), the runtime can't seed it deterministically —
Z3 picks an initial value on tick 0. Author intent: declare a
nullary first variant if you want a specific initial state.

```evident
enum CountState =
    InitialZero            -- nullary first variant
    Counting(Int)
```

## Performance tips

  * **Solve cost grows with body size.** A complex transpiler
    that runs every tick is expensive. Move it to a setup FSM
    that halts; render gets the cheap solve.
  * **Self-feedback effects re-schedule the FSM.** Even
    `Println("tick")` causes a re-tick. Use it intentionally;
    avoid emitting effects when there's nothing to do.
  * **Plugin ticks are paced by the OS thread sleep**, not Z3
    solves. Setting `EVIDENT_TICK_MS=10` doesn't make Z3 solve
    faster — it just produces more wake events.
  * **Profile with `EVIDENT_LOOP_TIMING=1`.** Per-FSM solve time
    + tick count breakdown shows which FSM is expensive.

## Debugging

  * **`EVIDENT_LOOP_TRACE=1`** — per-tick log of every FSM solve,
    state transitions, and skip decisions.
  * **`EVIDENT_LOOP_TIMING=1`** — solve/dispatch time breakdown.
  * **`EVIDENT_FFI_TRACE=1`** — every FFI call's arguments and
    return value.
  * Programs that "loop forever printing the same thing" almost
    always indicate effect-feedback without gating. Add a
    sequence counter or state tracker.

## See also

  * [`schema-interface.md`](../design/schema-interface.md) — the
    underlying model.
  * [`fsm-subscriptions.md`](../design/fsm-subscriptions.md) — the
    scheduler.
  * [`multi-fsm.md`](../design/multi-fsm.md) — composition patterns.
  * [`effect-state-machines.md`](effect-state-machines.md) — how to
    write a single FSM (the building block).
  * [`ffi-bindings.md`](ffi-bindings.md) — adding FFI surface for
    custom C libraries.
