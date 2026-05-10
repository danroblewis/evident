# Multi-FSM execution: separate state machines, shared world

## Motivation

The current effect-driven runtime has a strict 1:1:1 model:

  * **One** `main` claim per program.
  * **One** Z3 solver state (cached per-schema).
  * **One** per-step solve that produces all effects for that tick.

This makes structurally distinct concerns share solver work even
when they're causally independent within a frame. A 2D game that
declares its world as `Set Enemy + Set Bullet + player`, computes
gameplay logic, AND emits per-frame draw calls all in one main has
Z3 reasoning about the entire constraint system on every solve —
even when the rendering path's only "decision" is "what's the
current draw color".

The headline numbers from
`docs/plans/02-plugin-migrations/06-sdl-followups.md`:

```
effect_sdl_red.ev            (raw FSM, 1 shape)         1.7ms/step solve
effect_scene_yellow_box.ev   (declarative, 5 shapes)     93ms/step solve   ← 50× slower
effect_gl_transpiled_triangle.ev  (transpiler in main)  242ms/step solve   ← 142× slower
```

The slow paths aren't slow because they need solver power; they're
slow because constraints that don't depend on per-step state
(`render_items` walking a fixed list, `emit_shader` walking a
fixed AST) are re-translated and re-solved every step alongside
constraints that do.

Splitting the program into **multiple FSMs that share state but
solve independently** is the natural answer:

  * The render FSM has tiny per-frame state (frame counter, GL
    handles), and almost no decisions to make.
  * The game FSM has the gameplay logic — but doesn't care about
    the renderer's draw-call effects at all.
  * Their solvers each see a much smaller constraint system.

This document specs the runtime change.

## The shape

A program declares one or more **named FSMs**, each of which
follows the existing main-shape contract (state pair, last_results,
effects). FSMs share read-write access to a single named **World**
record. Convention:

```evident
-- Shared state — both FSMs can read it; the runtime serializes
-- writes (see "ownership" below).
type World
    player_pos ∈ IVec2
    enemies    ∈ EnemyList
    score      ∈ Int

-- Each FSM is a top-level claim with its own state pair.
claim game(world, world_next ∈ World,
           game_state, game_state_next ∈ GameState,
           last_results ∈ ResultList,
           effects ∈ EffectList)
    -- Per-tick logic: input + world → world_next, plus any
    -- side-effect requests (audio, save).

claim render(world ∈ World,
             render_state, render_state_next ∈ RenderState,
             last_results ∈ ResultList,
             effects ∈ EffectList)
    -- Pure: world is INPUT only. Produces draw effects per frame.
```

The runtime discovers the FSMs the same way it discovers `main`
today (state-pair detection), but accepts N of them instead of
exactly one.

### Why "world" specifically (and why a record)

Shared mutable state needs:

  * **Stable identity across FSMs** — both FSMs must agree on what
    `world.player_pos` means. A record (`type World`) gives that.
  * **Snapshottable** — the runtime needs to read the world out of
    the writer FSM's solve, then pin it as `given` for the reader
    FSMs' solves. Record-flat-expansion gives a clean field-by-field
    binding shape (we already do this for sub-schemas).
  * **Update-as-replacement** — the writer's `world_next` is the
    new world; readers next frame see that whole record. No partial
    updates, no concurrency invariants to write down.

## Execution order per tick

Each tick:

  1. Determine the **writer** for the world — the FSM that has a
     `world_next` Membership. (Multi-writer is rejected at load
     time; see Open Questions.)
  2. Solve the writer's FSM. Pin `world` to last tick's world
     value (or initial values if tick 0). Read `world_next` from
     the model.
  3. For each **reader** FSM (no `world_next`, but has `world` as
     input), solve it with `world` pinned to the writer's *new*
     `world_next`.
  4. For each FSM, dispatch its effects in the order they appear
     in `effects`. Effects across FSMs concatenate writer-first,
     readers in declaration order.
  5. Each FSM's `last_results` next tick = the results from ITS
     OWN dispatched effects (not the global pool).
  6. Halt when all FSMs report `state == state_next AND effects == ⟨⟩`.

Per-FSM `last_results` keeps the effect-result wiring local — a
render FSM that polls input via FFI doesn't accidentally consume
results meant for the game FSM.

### Effect ordering across FSMs

Conservative default: **writer first, readers in declaration
order**. This matches a typical game loop where:

  * Audio (owned by writer) issues sample queueing.
  * Render (reader) issues draw calls AFTER world updates have
    landed in the world snapshot.

If a use case needs custom ordering, a future extension could let
the program declare it (e.g. a `priority ∈ Int` claim convention).
For v1: no priority, fixed order.

## Comparison to MainCoordinator

`stdlib/main_coordinator.ev` already lets a program **swap entire
programs** while preserving a `world.*` state bundle. That's a
PROGRAM transition (menu.ev → gameplay.ev → endscreen.ev).

Multi-FSM is orthogonal: it's about splitting **one running
program** into multiple solvers. The two compose cleanly — a
single program can have multiple FSMs AND opt into program swaps
via `..MainCoordinator`. The world bundle that survives swaps is
the same world that's shared between FSMs.

## API surface for the user

Discovery is by convention:

  * Any top-level claim that has the main-shape MEMBERSHIP set
    (`state`/`state_next`/`last_results`/`effects` quadruple) is
    an FSM.
  * If exactly one claim is named `main`, the runtime keeps the
    current behavior (single-FSM, no shared-world handling).
  * If multiple FSM claims exist, the multi-FSM scheduler kicks
    in. There is no `main` — each FSM is named for its concern
    (`game`, `render`, `audio`).

Backwards-compat: existing single-`main` programs work unchanged.

### Naming

The runtime doesn't care about the FSM names beyond uniqueness.
Conventional names for documentation purposes:

  * `game` / `world_logic` — the writer; advances simulation.
  * `render` — produces draw effects from world state.
  * `audio` — produces sound effects from world state.
  * `input` — polls input devices, writes to world.

Programs are free to use any names; the load-time multi-FSM
detection only checks the membership shape.

## Implementation outline

### Detection

Extend `effect_loop::detect_main_shape` to:

  * Walk every top-level claim, not just `main`.
  * For each, run the existing membership-shape check.
  * Return a `Vec<MainShape>` with FSM names attached.
  * If 0: error (no FSMs found).
  * If 1: existing single-FSM path.
  * If ≥ 2: validate that exactly one writes `world_next`; build
    a multi-FSM execution plan.

### Per-FSM cache

Each FSM gets its own entry in `EvidentRuntime::cache` (the
existing `HashMap<String, CachedSchema>`). The `build_cache` path
already handles this — no change needed there.

### Per-tick orchestration

A new `effect_loop::run_multi_fsm` function:

```rust
fn run_multi_fsm(
    rt: &EvidentRuntime,
    fsms: &[MainShape],
    opts: &LoopOpts,
    ctx: &mut DispatchContext,
) -> Result<LoopResult, String>
```

Maintains `Vec<Option<Datatype>>` for each FSM's `current_state`
and `Vec<Vec<EffectResult>>` for each FSM's `last_results`.

Per tick:
  1. Encode the world record into a per-FSM `given` map.
  2. For the writer: solve, decode `world_next` from the model.
  3. For each reader: solve with `given.world.*` pinned to the
     writer's new `world_next.*`.
  4. Dispatch each FSM's effects, capture results, store per-FSM.
  5. Update each FSM's `current_state` from its own `state_next`.

### World encoding

Records flat-expand to per-field bindings (already supported via
sub-schema composition). Writing world from solve A → reading
world in solve B reuses the existing `given` mechanism with
`world.field_name` keys.

For nested records (e.g. `world.player.pos.x`), the dot-prefix
extraction recurses naturally — same machinery as the current
`Var::PinnedInt`-style propagation.

### Halting

The single-FSM halt detection (`state == state_next AND effects ==
⟨⟩`) generalizes to: ALL FSMs must report fixpoint AND empty
effects on the same tick.

## Open questions

### One writer per world field, or per world?

v1 picks **one writer for the whole world** for simplicity. Means:
  * The `game` FSM owns ALL of world's fields.
  * Render / audio / input are read-only.

For input handling — the input FSM may want to write to world
(e.g. `world.cursor_pos`). Two options:
  * **Multi-writer-with-disjoint-fields**: each writer declares
    which fields it owns; the runtime checks disjointness at load
    time. More complex but more flexible.
  * **Single-writer-with-input-channel**: input FSM produces an
    `Input` effect that the game FSM reads via `last_results`.
    Less elegant but uses existing primitives.

v1 punts: single writer. Input handling is via the second option
above.

### Are effects from different FSMs visible to each other's `last_results`?

v1 says NO — each FSM's `last_results` next tick = its own
effects' results. Simpler, fewer race conditions, matches the
"separate concerns" intent.

If we need cross-FSM communication, the world record IS that
channel. Add a field, the writer updates it, readers see the new
value next tick.

### Solve-order dependencies

Reader FSMs see the writer's `world_next` from THIS tick (one-tick
freshness). If the writer's solve fails, what do readers see?

v1: writer must succeed for the tick to proceed. If the writer's
solve is UNSAT, the whole runtime halts with the same error path
as the single-FSM case.

### What if two FSMs share the same `last_results` namespace by accident?

The check at load time: every FSM has its own `last_results`
membership. They're always distinct because they're declared in
different claims. Cross-FSM result leakage isn't possible by
construction.

### Initial world value

For tick 0, the writer's solve sees `world` unbound (initial
fields can be set via the writer's body's constraints). This is
the same as how a single-FSM `main` initializes its state today.

### Observability

EVIDENT_LOOP_TIMING (just added) needs to print per-FSM timing:

```
[timing] tick 5: game solve=8.1ms render solve=2.3ms dispatch=33ms
```

So users can see which FSM is the bottleneck.

## Migration: how existing programs adopt this

Single-FSM programs get NO change — `main` keeps working.

To split a single-FSM program into game + render:

  1. Move drawing-related logic out of `main` into a new `render`
     claim. Move per-frame draw-call construction with it.
  2. Move logic-related logic into a new `game` claim.
  3. Define a `World` type holding the game's persistent state.
  4. `game` writes `world_next` from per-tick logic.
  5. `render` reads `world` (no `world_next`) and produces draws.
  6. Delete `main`.

The user-facing rewrite is a refactor, not a redesign. Renderable
list construction in render's body stays the same shape; the only
thing that moves is which solver context owns it.

## What this enables

Once multi-FSM lands:

  * Per-FSM cache eviction — game changes don't bust the
    rendering cache.
  * Per-FSM timing budgets — render hard-bounded to 16ms for
    60fps; game has whatever's left.
  * Future: parallel solves where FSMs are independent (game and
    audio could solve simultaneously).
  * Cleaner stdlib library shape — `..SDLScene` becomes a render
    FSM, not "the whole main". Users add their own game FSM
    alongside.

## Out of scope (v1)

  * **Multi-writer worlds** — see Open Questions.
  * **Inter-FSM message channels** — use the world record.
  * **Parallel FSM execution** — sequential per-tick; parallelism
    is a perf optimization layered later.
  * **Per-FSM tick rates** — every FSM advances every tick. Fixed
    timestep simulation (game runs at 60Hz, render at 30Hz) is a
    separate concern that can be layered via per-FSM tick
    counters in the writer's world.

## Sketch of a worked example

```evident
import "stdlib/sdl/scene.ev"

type World
    player_pos ∈ IVec2
    score      ∈ Int

enum GameState =
    InMenu
    Playing(Int)        -- ticks_in_match
    GameOver(Int)       -- final score

enum RenderState =
    SetupRender
    Frame(Int, Int)     -- (window, renderer)

-- Game FSM: owns the world.
claim game(world, world_next ∈ World,
           game_state, game_state_next ∈ GameState,
           last_results ∈ ResultList,
           effects ∈ EffectList)

    -- Initial conditions
    game_state = InMenu ⇒ (world_next.player_pos = IVec2(320, 240) ∧
                            world_next.score = 0)

    -- ... gameplay logic ...
    -- ... transitions ...
    effects = ⟨⟩    -- game has no effects in this slim example


-- Render FSM: reads world, draws.
claim render(world ∈ World,
             render_state, render_state_next ∈ RenderState,
             last_results ∈ ResultList,
             effects ∈ EffectList)

    items ∈ RenderableList
    items = ⟨
        RFilledRect(world.player_pos.x, world.player_pos.y, 32, 32,
                    255, 200, 0, 255)
    ⟩

    ..SDLScene
```

Two solves per tick — game advances world, render produces
effects. Each is a tiny constraint system because each FSM only
has its own concerns to reason about.
