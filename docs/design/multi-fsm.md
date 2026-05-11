# Multi-FSM execution: separate state machines, lifecycle phases, shared world

> **2026-05-09 update**: this doc was the original framing for
> "multi-FSM" in evident. It's still a useful overview of the
> writer/reader composition pattern and worked examples, but the
> halt mechanism described below has been **superseded** by
> subscription-driven scheduling. Read these in order:
> 1. [`schema-interface.md`](schema-interface.md) — what an Evident
>    model IS (the unified model: read-set + write-set + private
>    state + schedule + behavior).
> 2. [`fsm-subscriptions.md`](fsm-subscriptions.md) — the scheduler
>    that implements the unified model.
> 3. This doc — composition patterns and worked examples (still valid).
>
> Specifically: the "halt-per-FSM is the load-bearing semantic"
> section's name/fixpoint halt heuristic is gone. Halt is now
> implicit ("no FSM scheduled in a tick = halt"). Plugins are
> first-class schemas (FrameTimer, SigintSource, StdinSource);
> they write world fields and other FSMs subscribe via standard
> read-set inference.

## Motivation

The current effect-driven runtime has a strict 1:1:1 model:

  * **One** `main` claim per program.
  * **One** Z3 solver state (cached per-schema).
  * **One** per-step solve that produces all effects for that tick.

This forces structurally distinct concerns to share solver work
even when they're causally independent. The headline numbers from
`docs/plans/02-plugin-migrations/06-sdl-followups.md`:

```
effect_sdl_red.ev            (raw FSM, 1 shape)         1.7ms/step solve
effect_scene_yellow_box.ev   (declarative, 5 shapes)     93ms/step solve   ← 50× slower
effect_gl_transpiled_triangle.ev  (transpiler in main)  242ms/step solve   ← 142× slower
effect_gl_uniform_triangle.ev     (animated, complex AST) 789ms/step solve  ← 460× slower
```

These slow paths aren't slow because they need solver power; they're
slow because constraints that don't depend on per-step state
(`render_items` walking a fixed list, `emit_shader` walking a
fixed AST, `setup_seq = LibCall(SDL_Init)`) are re-translated and
re-solved every step alongside constraints that do.

### GL is itself a state machine on remote hardware

The CPU role in OpenGL is "configure GPU state once, then per
frame: update uniforms + issue draw calls." Vertex transform and
fragment shading happen on a *different machine* (the GPU). The
proper architecture matches this:

  * **Once at startup**: compile shaders, upload textures, configure
    vertex buffers, link programs. These results live in GPU memory
    and persist for the program's lifetime.
  * **Each frame**: glUseProgram + glUniform* + glDraw*. Microscale
    CPU work; the GPU does the heavy lifting.

What we do today: the equations defining the setup phase
(`sdl_init_eff = LibCall(...)`, `vertex_src = emit_shader(ast)`)
live in `main`'s body forever. Even after the state machine
transitions to `Frame`, those equations are part of every solve.
Z3 re-derives the same string from the same AST 90 times for a
90-frame demo. The transpiler is "logically dead code" by frame 1
but stays alive in the constraint system.

Splitting the program into **multiple FSMs that share state and
have independent lifecycles** matches GL's architecture:

  * The **setup FSM** runs once: pushes shader/texture/vertex state
    to the GPU, captures the resulting handles into shared world,
    then **halts**. Never solved again.
  * The **render FSM** has a tiny constraint set: `compute uniform
    value from world; emit Seq(set_uniform, draw, swap, pump,
    delay)`. Microseconds per solve.
  * Optional **game FSM**: gameplay logic, advances world.
  * Optional **input FSM**: polls input, writes to world.

Each FSM advances at its own pace. Halted FSMs stop being solved.
The world record is the sole channel for shared state.

## Lifecycle: halt-per-FSM is the load-bearing semantic

> **Note (2026-05-09)**: The halt mechanism described in this section
> is being replaced by subscription-driven scheduling. See
> [`fsm-subscriptions.md`](fsm-subscriptions.md) for the new model.
> The rest of this doc (writer/reader, world composition, examples)
> remains accurate.

The single-FSM runtime treats halt as program termination
(`state == state_next AND effects == ⟨⟩` → exit cleanly). In the
multi-FSM design, halt is **per-FSM**: when one FSM hits the halt
fixpoint, the runtime stops solving it but keeps advancing the
others.

This makes the GL setup-then-halt pattern the canonical "do
something once" mechanism:

```
enum SetupState =
    BootSDL                  -- emit setup_seq, capture handles
    StoreHandles             -- write window + ctx to world
    Done                     -- halt forever

claim setup(world, world_next ∈ World, ...)
    state_next = match state
        BootSDL          ⇒ StoreHandles
        StoreHandles     ⇒ Done
        Done             ⇒ Done
    -- After Done is reached + effects = ⟨⟩, this FSM is dropped
    -- from the per-tick scheduler. Its world contributions persist.
```

Once `setup` halts, the `vertex_src = emit_shader(...)` equation
that lives in setup's body is **gone from per-tick solves**. The
transpiler runs once during setup's body inlining, never again.

The render FSM, which receives the GL handles from world, has only
the tiny per-frame logic in its constraint set. That's the perf fix.

### What "halt" means precisely

An FSM halts when, on a single tick:

  1. Its `state_next` value equals its `state` value (fixpoint), AND
  2. Its emitted `effects` list is empty.

Both must be true on the SAME tick. An FSM that emits effects but
also has `state_next == state` (e.g., still pushing a continuous
audio stream) is NOT halted.

After halt:

  * The FSM is removed from the per-tick scheduler.
  * Its solver state can be dropped (cache eviction).
  * Its last `world_next` (if it's the writer) is the world the
    other FSMs continue to read from on subsequent ticks.
  * The FSM does NOT re-enter even if some condition changes — halt
    is permanent. Programs that need "wake on event" should use a
    long-running FSM that has an Idle state, not a halt-then-resume
    pattern.

### Program-level halt

The program halts when **all** FSMs report halt on the same tick.
Single-FSM programs continue to use the existing single-FSM halt
detection — backwards-compat is exact.

## Shape

A program declares one or more **named FSMs**, each of which
follows the existing main-shape contract (state pair,
`last_results`, `effects`). FSMs share read access to a single
named **World** record; one FSM at most writes to it.

```evident
type World
    -- Shared state — written by the writer FSM, read by all.
    window     ∈ Int                    -- SDL window handle
    renderer   ∈ Int                    -- SDL renderer handle
    program    ∈ Int                    -- GL program handle
    time_loc   ∈ Int                    -- uniform location
    player_pos ∈ IVec2
    score      ∈ Int

claim setup(world, world_next ∈ World,
            setup_state, setup_state_next ∈ SetupState,
            last_results ∈ ResultList,
            effects ∈ EffectList)
    -- … push GL state, capture handles into world_next …
    -- Halts when state reaches Done.

claim game(world, world_next ∈ World,
           game_state, game_state_next ∈ GameState,
           last_results ∈ ResultList,
           effects ∈ EffectList)
    -- … gameplay logic, mutates world …

claim render(world ∈ World,                    -- READ-ONLY
             render_state, render_state_next ∈ RenderState,
             last_results ∈ ResultList,
             effects ∈ EffectList)
    -- … push uniforms based on world; emit draws …
```

The runtime discovers FSMs by walking top-level claims and
matching the membership shape (state pair + `last_results` ∈
ResultList + `effects` ∈ EffectList). Single-`main` programs
continue to use the single-FSM path.

### One writer per world

v1: at most one FSM declares `world_next ∈ World`. The runtime
checks this at load time and rejects programs with multiple
writers.

Two or more FSMs CAN write IF the runtime can prove their writes
are field-disjoint. v1 doesn't do this proof; if you need
multi-writer, structure as a single writer (the "game") that reads
"input requests" via `last_results` from sibling FSMs.

### Per-FSM `last_results`

Each FSM's `last_results` next tick contains ONLY the results
from its OWN dispatched effects on the previous tick. No cross-FSM
result leakage. Cross-FSM communication is through the world record.

## Execution order per tick

Per tick, the scheduler:

  1. **Snapshot world** from last tick (initial values for tick 0).
  2. **Solve writer** (if there is one and it's not halted): pin
     `world.*` from the snapshot. Read `world_next.*` from the
     model. The new world becomes the current snapshot.
  3. **Solve readers** (in declaration order, skipping halted ones):
     pin `world.*` from the (possibly-just-written) snapshot.
  4. **Dispatch effects** in order: writer first, readers in
     declaration order.
  5. **Per-FSM result capture**: each FSM's `last_results` for next
     tick = its own dispatched effects' results.
  6. **Per-FSM halt detection**: any FSM that reports
     `state_next == state ∧ effects == ⟨⟩` is dropped from the
     scheduler.
  7. **Program halt**: when no FSMs remain in the scheduler, exit.

If on a tick, the writer hasn't halted but emits no `world_next`
update (i.e. its `world_next` equals the snapshot), readers still
run with the snapshot from last tick. No special case needed.

### Handoff at writer halt

When the writer FSM halts:

  * Its last successfully-solved `world_next` becomes the
    permanent world for all readers' subsequent ticks.
  * Readers continue to see that frozen world; they can't mutate it
    (they were never writers).
  * If an active FSM needs to BECOME the writer (e.g., game takes
    over after setup halts), the load-time validation already
    permits this — `setup` writes `world_next ∈ World`, `game`
    also writes `world_next ∈ World`. Multi-writer-validation
    needs to check non-overlapping ACTIVE windows: setup writes
    only while running; game writes only when setup is done. v1:
    DON'T allow this; require all writers to be live concurrently.

For v1, the simplest pattern is: **setup writes to a SubsetOfWorld
type that includes only the handles; game writes to a different
SubsetOfWorld**. No overlapping fields. Implementation: a single
`world` record where setup writes some fields, game writes others;
the runtime checks at load time that each field has only one
writer.

This is the natural shape — the GL handles are filled by setup,
gameplay state is filled by game. They don't conflict.

## State semantics

**World**: shared, multiple-readers / single-write-per-field. Lives
forever — even after the writer halts, its last value persists for
readers.

**Per-FSM `state`**: private to that FSM. The scheduler tracks each
FSM's current state value across ticks the same way single-FSM
mode does.

**Per-FSM `last_results`**: private. Each FSM only sees results
from its own effects.

**Effects**: globally ordered (writer first, then readers in
declaration order) but their RESULTS go back to the originating
FSM only.

## Comparison to MainCoordinator

`stdlib/main_coordinator.ev` lets a program **swap entire
programs** while preserving a `world.*` state bundle. That's a
PROGRAM transition (menu.ev → gameplay.ev → endscreen.ev).

Multi-FSM is orthogonal: it's about splitting **one running
program** into multiple solvers with independent lifetimes. The
two compose cleanly — a single program can have multiple FSMs AND
opt into program swaps.

## Worked examples

The implementation should make all of these work. They're written
as the user would author them — the multi-FSM scheduler is an
implementation detail.

### Example 1: minimal two-FSM (game + render, both loop)

The simplest interesting case. Game owns world, render reads it.

```evident
import "packages/sdl/scene.ev"

type World
    player_pos ∈ IVec2
    score      ∈ Int

enum GameState =
    Init
    Playing
    Done

claim game(world, world_next ∈ World,
           game_state, game_state_next ∈ GameState,
           last_results ∈ ResultList,
           effects ∈ EffectList)
    state_next = match state
        Init     ⇒ Playing
        Playing  ⇒ Playing       -- never halts (real game has Done)
        Done     ⇒ Done

    -- Initial world; subsequent ticks just hold steady.
    world_next.player_pos = (state = Init ? IVec2(320, 240)
                                          : world.player_pos)
    world_next.score      = (state = Init ? 0 : world.score)
    effects = ⟨⟩

claim render(world ∈ World,
             render_state, render_state_next ∈ RenderState,
             last_results ∈ ResultList,
             effects ∈ EffectList)
    -- Renders one rect at world.player_pos. Setup happens INSIDE
    -- this FSM (sub-optimal — see Example 2 for the right way).
    items ∈ RenderableList
    items = ⟨RFilledRect(world.player_pos.x, world.player_pos.y, 32, 32,
                         255, 200, 0, 255)⟩
    title      = "Player"
    width      = 640
    height     = 480
    frames     = 90
    background = Color(20, 20, 30, 255)
    ..SDLScene
```

Validates: per-FSM scheduling, world handoff (game writes,
render reads), per-FSM `last_results` isolation.

### Example 2: setup + render (lifecycle, the GL killer case)

The headline use case. Setup pushes GL state once, halts.
Render runs forever with a tiny constraint set.

```evident
import "packages/sdl/gl.ev"
import "packages/gl/program.ev"
import "packages/glsl/transpile.ev"

type World
    window   ∈ Int
    ctx      ∈ Int
    program  ∈ Int
    time_loc ∈ Int
    vao      ∈ Int

enum SetupState =
    BootSDL
    AwaitWindow
    -- … existing GL setup chain …
    StoreHandles
    Done

enum RenderState =
    Idle               -- waiting for setup to finish
    Frame
    Quit

claim setup(world, world_next ∈ World,
            setup_state, setup_state_next ∈ SetupState,
            last_results ∈ ResultList,
            effects ∈ EffectList)
    -- … uses Effect::Seq + ArgPriorResult to chain SDL_Init →
    --   CreateWindow → CreateContext → MakeCurrent → glViewport →
    --   GenVertexArrays → CreateShader×2 + transpiler → LinkProgram →
    --   GetUniformLocation, captures all handles into world_next.
    --
    -- This FSM HALTS at Done. The transpiler's `emit_shader(ast,
    -- src)` equation lives in this body — gone from per-tick solves
    -- the moment setup halts.

claim render(world ∈ World,
             render_state, render_state_next ∈ RenderState,
             last_results ∈ ResultList,
             effects ∈ EffectList)
    -- Render only runs after setup is done. Detect via
    --   world.program ≠ 0     (zero is the unset sentinel)
    -- (This convention works because GL handles are always positive.)
    setup_done ∈ Bool
    setup_done = (world.program ≠ 0)
    state_next = match state
        Idle ⇒ (setup_done ? Frame : Idle)
        Frame ⇒ Frame
        Quit ⇒ Quit
    -- Per-frame: push uniform, draw, swap.
    time_val ∈ Real
    time_val = ... -- some function of frame counter (could come from world too)
    set_uniform_eff ∈ Effect
    gl_uniform_1f(world.time_loc, time_val, set_uniform_eff)
    -- … other per-frame effects …
    effects = match state
        Frame ⇒ ⟨set_uniform_eff, draw_eff, swap_eff, pump_eff, delay_eff⟩
        _     ⇒ ⟨⟩
```

Validates: lifecycle halt, transpiler lives in halted FSM only,
per-frame solve is microseconds.

### Example 3: three-FSM (setup, game, render)

Combines lifecycle + concurrent subsystems. Setup runs once for
GL state, game runs forever for gameplay, render runs forever for
drawing. All share the world.

```evident
type World
    -- Setup-owned (written by setup, read-only after halt)
    renderer   ∈ Int
    program    ∈ Int
    -- Game-owned (written by game every tick)
    player_pos ∈ IVec2
    score      ∈ Int

claim setup(world, world_next ∈ World, ...)
    -- writes world_next.renderer, world_next.program; halts.
    -- world_next.player_pos / score = world.player_pos / score
    -- (passes through unchanged so game-owned fields survive).

claim game(world, world_next ∈ World, ...)
    -- writes world_next.player_pos, world_next.score every tick.
    -- world_next.renderer = world.renderer (passthrough).

claim render(world ∈ World, ...)
    -- read-only; pushes uniforms from world, draws.
```

Note: this requires field-disjoint multi-writer support. v1 punts;
v1.5 fixes.

### Example 4: sibling FSMs that don't share world

Sometimes there's no shared state — two independent state machines
just happen to coexist. v1 should support this trivially.

```evident
claim ticker(state, state_next ∈ TickerState,
             last_results ∈ ResultList, effects ∈ EffectList)
    -- prints to stdout every step

claim heartbeat(state, state_next ∈ HeartbeatState,
                last_results ∈ ResultList, effects ∈ EffectList)
    -- writes to a file every step
```

No World type at all. Both FSMs run forever on their own schedule.
Validates: World is optional.

### Example 5: halt-then-program-halt

When all FSMs halt, the program exits.

```evident
claim countdown_a(state, state_next ∈ CDState, ...)
    -- counts to 5 then halts

claim countdown_b(state, state_next ∈ CDState, ...)
    -- counts to 3 then halts
```

After tick 3, countdown_b halts. After tick 5, countdown_a halts.
The program exits after tick 5.

Validates: per-FSM halt ≠ program halt; program halt only when
ALL FSMs are halted.

## Test plan

Concrete behaviors that must validate the implementation. Each
should be expressible as a `tests/lang_tests/multi_fsm/*.ev`
file or a Rust integration test in `runtime/tests/`.

### Detection

  * **single_main_unchanged**: a program with one `main` claim
    runs through the existing single-FSM path. No behavior change.
  * **two_fsm_detected**: program with `game` + `render` claims
    (each main-shape) is detected as multi-FSM. Both run.
  * **non_main_named**: claims aren't required to be named `main`
    or anything specific — any top-level claim with the membership
    shape qualifies.
  * **mixed_shapes_rejected**: a claim that has SOME of the
    membership shape but not all (e.g., `effects` but no
    `last_results`) is NOT treated as an FSM. Either errors or is
    skipped.
  * **multiple_writers_rejected**: program with two FSMs both
    declaring `world_next ∈ World` errors at load time
    (in v1, until field-disjoint multi-writer lands).

### Per-tick scheduling

  * **writer_first**: in a tick, the writer FSM is solved before
    any reader. Verify by an effect-list ordering test
    (writer's effects appear first in dispatch trace).
  * **reader_sees_new_world**: writer increments
    `world_next.counter`; reader on the SAME tick reads the
    incremented value. Verify by reader emitting `Println(...)`
    of the value.
  * **per_fsm_last_results**: writer emits `Time` effect (returns
    Int ms). Reader emits `Println("hello")`. On next tick,
    writer's `last_results` has the Int, reader's `last_results`
    has the NoResult. NO cross-leakage.

### Lifecycle

  * **single_fsm_halt**: in multi-FSM mode, a single FSM halts
    individually. Verify the runtime detects the halt and stops
    solving it.
  * **halted_world_persists**: setup writes `world.x = 42`, halts.
    Reader on subsequent ticks still sees `world.x = 42`.
  * **all_halt_program_exit**: when every FSM halts, the program
    exits cleanly with the same `LoopResult` shape as
    single-FSM does.
  * **halt_doesnt_resume**: an FSM that halts on tick N stays
    halted on tick N+1 even if its `state == state_next ∧ effects
    == ⟨⟩` predicate would now be false (e.g., its state field is
    bound to depend on world, and world changed).

### Effect ordering + dispatch

  * **effect_order_writer_then_readers**: dispatch trace shows
    writer's effects before readers' effects, readers in
    declaration order.
  * **per_fsm_effect_dispatch**: each FSM's effects are dispatched
    via the same `dispatch_all` path; result types match each FSM's
    `last_results` decoder.

### Solve-cost (perf regression test)

Not a correctness test, but the runtime should expose timing such
that we can assert: in a setup+render program, after the setup
FSM halts, the render FSM's per-step solve is < 5ms. This catches
"oh oops the transpiler is somehow still in the render FSM's
constraint set".

**Validated 2026-05-09**:

| Demo                                                | Steady solve / step |
|---|---|
| `effect_gl_uniform_triangle.ev` (single FSM)        | ~464ms |
| `effect_multi_fsm_triangle.ev`  (setup + render, hardcoded shaders) | ~2.6ms |
| `effect_gl_transpiled_triangle.ev` (single FSM, transpiler) | ~218ms |
| `effect_multi_fsm_transpiled.ev` (setup + render, transpiler) | ~3ms |

Roughly **180×** speedup on the uniform demo and **70×** on the
transpiler-in-setup demo. Setup runs the full SDL+GL init chain
(one Seq, 25 calls with `ArgPriorResult` threading) plus — in the
transpiled variant — the GLSL transpiler over both shader ASTs.
Setup pays ~700ms per tick across 3 ticks, then halts and is
dropped from the scheduler. Render reads handles from `world` and
emits `glUniform1f + glClear + glDrawArrays + swap + pump + delay`
each frame. Wall time per frame is dominated by `SDL_Delay(33)`
frame pacing (~26 fps).

### World encoding

  * **world_field_passthrough**: writer's body writes only some
    fields; the runtime correctly preserves unwritten fields from
    the previous tick. (Or rejects partial writes, depending on
    semantics — TBD.)
  * **nested_record_world**: `World { player ∈ Player(IVec2, Int) }`
    — recursive flat-expansion still works.

### Backwards compat

  * Every existing demo / test continues to pass without
    modification. The runtime takes the multi-FSM path only when N
    > 1 FSMs are detected.

## Implementation outline

### Phase 1: detection + execution

  * Extend `effect_loop::detect_main_shape` to walk every top-level
    claim. Return `Vec<MainShape>`.
  * Identify writer vs reader by checking `world_next` membership.
  * Validate single-writer (or no-world) programs accept; reject
    others with a clear error.
  * Add `effect_loop::run_multi_fsm` that mirrors
    `run_with_shape` but iterates per FSM each tick.
  * Use existing `EvidentRuntime::cache` (already keyed by claim
    name; works as-is for multiple FSMs).

### Phase 2: lifecycle

  * Per-FSM halt tracking (a `Vec<bool>` parallel to the FSM list).
  * Skip halted FSMs in the per-tick solve loop.
  * Cache eviction for halted FSMs (free their solver to reduce
    memory; not strictly needed for correctness).
  * Program exits when all FSMs halted.

### Phase 3: world handoff

  * Encode/decode the World record across solves: writer's
    `world_next.*` becomes the next tick's `world.*` for everyone.
  * Field-level snapshotting using the existing flat-expansion
    machinery.

### Phase 4: timing / observability

  * Extend `EVIDENT_LOOP_TIMING` to print per-FSM timing.
  * Per-FSM cache statistics for debugging slow FSMs.

## Open questions

### How does setup hand off to game when setup halts?

In Example 3, setup writes `world.renderer` then halts. Game
writes `world.player_pos` every tick. They both want to be
"writers" — but at different times.

**v1**: Reject this; one writer per program for the whole run.
Setup-then-render works (only setup writes; render reads forever).
For setup-then-game-then-render, restructure so game is the
permanent writer, calls into setup logic via state.

**v1.5**: Allow field-disjoint multi-writers. The runtime computes
which fields each FSM declares in `world_next.field = …` and
checks no field is written by multiple FSMs.

**Future**: Allow time-disjoint writers (setup writes only while
running; game writes after setup halts).

### What if a reader needs to write?

Use the world record. Reader emits an effect; on next tick its
`last_results` has the result; the writer reads… wait, no, FSMs
don't see each other's `last_results`.

The pattern: **reader writes to a "request queue" field in
world**. The writer reads that field, processes it, writes the
result. Game-loop pattern: input FSM writes `world.input_events
∈ EventList`, game FSM reads it, clears it on next tick.

### What about per-FSM tick rates?

V1: every FSM ticks every tick. Per-FSM tick rates (game at 60Hz,
render at 30Hz) are layered later via per-FSM tick counters in
world (writer increments every tick, render checks `world.tick %
2 == 0` before doing real work).

### Solve-order dependencies

If the writer's solve fails (UNSAT), what happens to readers?

V1: writer must succeed. Readers don't run. The runtime halts
with an error (same as single-FSM UNSAT today).

### Halted FSM resumption

V1: halt is permanent. If a use case wants "wake on event," use a
long-running FSM with an Idle state (not halt-then-resume).

This avoids the "did the halt detection see a TRANSIENT fixpoint
we shouldn't have halted on?" footgun.

### Does the world need to be immutable per tick?

V1 model: writer's `world_next` is computed atomically per tick
(it's the model output of one solve). Readers see that whole new
world or the previous tick's whole world — never a partial update.

This is naturally atomic because each solve produces one consistent
model.

## Out of scope (v1)

  * Multi-writer worlds (deferred to v1.5).
  * Inter-FSM message channels beyond the world record.
  * Parallel FSM execution (sequential per-tick; parallelism is a
    perf optimization layered later).
  * Per-FSM tick rates (manual via world.tick counter).
  * Halt-then-resume (use Idle state instead).
