# Program Coordination & Runtime Concurrency

## The Question

What does Evident do at scale? A 52-level game with menus, settings, and
saves doesn't fit in one constraint model. Even though `active = X ⇒ …`
gating makes only one level's constraints "fire" at a time, the model
itself contains all 52 levels' worth of constraints — paying their
translation cost on every load and their memory cost on every step.

This doc proposes a coarser unit of composition than schemas: the
**program**. Each level is its own Evident program (own file, own
constraint model). The runtime swaps between programs at well-defined
boundaries, carrying world state across the swap. Multiple programs
can run **concurrently** — not because the constraint solver got smarter,
but because separate programs are by construction independent solver
instances.

This complements two existing docs:

  - [`multi-schema-coordination.md`](./multi-schema-coordination.md)
    discusses how multiple **schemas** coordinate *within* one
    constraint model. Different problem.
  - [`synchronous-reactive-concurrency.md`](./synchronous-reactive-concurrency.md)
    argues against `async` / threads / channels at the **language**
    level. This doc agrees and proposes runtime concurrency at the
    **program** level — outside the language surface entirely.

---

## Programs as the Unit of Composition

A **program** in Evident is what `evident execute file.ev` runs: a
schema named `main`, plus whatever it imports and composes. Today,
the executor runs exactly one program per process. The proposal: let
the executor host many programs and switch between them.

```
levels/
  menu.ev
  level_01.ev      ← each is a normal Evident program
  level_02.ev
  ...
  level_52.ev
  game_over.ev
program_chooser.ev ← shared claim every level imports
world.json         ← initial state, deserialized into `given`
```

**Each program is its own constraint model.** Z3 sees only the
active program's constraints. Levels 02–52 don't exist in solver
state until they're loaded.

**State survives swaps via convention.** Any binding under `world.*`
is extracted from the current program's bindings on swap and passed
to the next program's first frame as `given`. Nothing else carries.
This is the explicit interface between programs — no implicit shared
solver state, no leaked Z3 internals.

---

## The Swap Signal

The executor watches one extra binding per step: `next_program`. It's
a normal Evident variable (string-typed) that any constraint can
write. The contract:

| `next_program` value | Executor behavior              |
|---|---|
| `""` (or unchanged)  | Stay in current program; advance state as usual |
| `"some/file.ev"`     | Swap: extract `world.*`, drop cache, load + initialize that program |
| `"halt"` (sentinel)  | Shut down the executor                          |

`next_program` is just `state_next.*`-style output: the program
writes it, the executor reads it after each solve. No new language
machinery — the variable participates in the constraint model like
any other.

### One solve per frame, not two

The naive design has a separate "manager" program running in
parallel with each level — one solve picks `next_program`, another
solve runs the level. That's 2× per-frame cost AND awkward
state-sharing.

Better: the chooser is a **claim** every level passthroughs.

```evident
-- stdlib/program_chooser.ev
claim ProgramChooser
    state         ∈ World     -- read whatever world fields you want
    next_program  ∈ String    -- output the executor watches
    -- Default: stay (the calling program's own constraints can override)
    -- Common transitions can live here too if shared across many programs.
```

```evident
-- levels/level_03.ev
import "stdlib/program_chooser.ev"

type main
    ..ProgramChooser     -- adds `next_program` field + base rules
    -- ... level-specific gameplay ...

    -- Local override:
    state.player.lives = 0 ⇒ next_program = "game_over.ev"
    state.player.score ≥ 1000 ⇒ next_program = "level_04.ev"
```

One solve per frame. The chooser is part of the same constraint
model so it can read level state directly through the passthrough.

---

## The Same Mechanism, Two Scales

The structural-signature cache rebuild we already implemented (see
[`runtime-and-io.md`](./runtime-and-io.md) and the
`structural_signature` machinery in `runtime-rust/src/translate/preprocess.rs`)
was for one program with changing structural givens — "did the
unroll count change?" → "yes, rebuild the cache against new given".

Program-swap is the same shape at a coarser grain — "did the
program change?" → "yes, rebuild the cache, AND the cache key now
includes the program identity".

The unification: **cache key becomes `(program_id, structural_signature)`**.

```rust
cache: HashMap<(ProgramId, StructuralSignature), CachedSchema>
// Lookup steps each frame:
//   compute (current_program, current_signature)
//   if cached, run_cached
//   else build_cache, store
```

`needs_rebuild` covers both kinds of change. `cache_rebuilds()`
counter reports both — so a perf observer sees "thrashing programs"
vs "thrashing signatures within one program" without separate
plumbing.

**LRU on top.** Keep the most recent N program caches warm in
memory; evict beyond N. Menu↔level back-and-forth is then instant
(both stay warm); a one-shot transition to level 47 for the first
time pays its translation cost.

---

## Where Concurrency Comes In

> Evident's constraint solves are sequential by nature: the SAT
> solver works on one model at a time, and `synchronous-reactive-
> concurrency.md` argues against parallelism *within* a model. But
> nothing about that argument extends to *between* models. Two
> programs that don't share solver state are by definition
> independent.

This is the architectural opening. With program-as-unit composition,
several patterns become natural:

### 1. Background simulations
A long-running simulation (e.g. AI behavior tree, physics
prediction) runs as its own program, in its own thread, with its
own Z3 context. Communicates with the foreground program by writing
to a shared world key (executor mediates). The foreground reads
the latest result whenever it needs to. No shared mutable state at
the language level — just the world handoff.

### 2. Multi-agent systems
A game with 8 NPCs could run each NPC's decision logic as its own
program. They share read-only access to `world.*`; each writes
back to `world.npc_<i>.*`. The executor runs them in parallel
(thread pool) at frame boundaries, then merges. Order-independence
is enforced by the world-namespacing.

### 3. UI + simulation split
The UI (menu, HUD) and the simulation (game world) are different
programs running at different rates. UI ticks at vsync; simulation
ticks at fixed dt. They share `world.*` (player input, score,
state). Each runs independently in its own thread; the executor
synchronizes at world-state read/write boundaries.

### 4. Cross-program speculative execution
For pre-loading, the executor could speculatively translate level
N+1 in a background thread while level N is running, so the swap
is instant. Pure perf optimization, transparent to the program.

### What enables this safely

  - **Z3 isolation per program.** The Rust runtime already leaks
    one `Context` per `EvidentRuntime`. Multiple runtimes →
    multiple contexts → no shared solver state. Z3 contexts are
    not safe to share across threads, but separate contexts are
    fine in separate threads.
  - **Explicit state contract.** `world.*` is the only shared
    namespace. Concurrent programs touch their own subsections.
    Conflicts are resolved by convention (last-writer-wins per
    field, or merge rules in the coordinator).
  - **No cross-program callbacks.** Programs talk via shared
    state, not invocation. There's no "program A calls into
    program B" — that would require shared Z3 state. They only
    READ each other's outputs through the world.

### What this is NOT

  - Not threads at the language level. `synchronous-reactive-
    concurrency.md` stands: no `async` / locks / channels in
    Evident source.
  - Not parallelism within one model. Z3 solves remain sequential
    within their program.
  - Not a shared-memory concurrency model. The world handoff is
    value-based — no pointers leak, no mutable references shared.

The model is closer to **actor-system-with-frame-sync** than
threading: each program is an actor, the executor is the scheduler,
the world is the message space.

---

## The Coordinator as a Real Concept

Putting the pieces together, the runtime grows a "coordinator"
layer that's distinct from any single Evident program:

```
  ┌────────────────────────────────────────────────────┐
  │                  Executor (Rust)                    │
  │  ┌──────────────────────────────────────────────┐  │
  │  │              Coordinator                      │  │
  │  │  - Active program registry                    │  │
  │  │  - World-state ownership + handoff            │  │
  │  │  - Cache (program_id, signature → schema)     │  │
  │  │  - LRU eviction policy                        │  │
  │  │  - Per-program scheduling (sync / parallel)   │  │
  │  └──────────────────────────────────────────────┘  │
  │  ┌──────────────┐  ┌──────────────┐  ┌──────────┐ │
  │  │ Program A    │  │ Program B    │  │ ...      │ │
  │  │ (Z3 ctx A)   │  │ (Z3 ctx B)   │  │          │ │
  │  └──────────────┘  └──────────────┘  └──────────┘ │
  └────────────────────────────────────────────────────┘
```

The coordinator owns:

  - **Program lifecycle.** Load on demand, evict on LRU pressure,
    drop on shutdown.
  - **World state.** Authoritative copy lives here; programs read
    it as `given`, write to it via `world_next.*` bindings.
  - **Scheduling.** Decides which programs to step this frame,
    in what order, in serial or parallel. Started serial; could
    grow to parallel without changing program-level semantics.
  - **Cache and signature tracking.** Same machinery as the
    existing structural-signature rebuild, generalized.

User-visible Evident programs see none of this. They see: their
own state, their own next_program, the world fields they read and
write. The fact that the executor might be running them in parallel
with three other programs is invisible to the program's own logic
— exactly because the program is a constraint model with no shared
mutable state by construction.

---

## What Stays in the Existing Doc Set

  - `synchronous-reactive-concurrency.md` is the source of truth
    for "no threads at the language level". This proposal doesn't
    contradict it — it adds runtime concurrency that's outside
    the language surface.
  - `multi-schema-coordination.md` is the source of truth for
    "how do multiple schemas talk inside one program". Still
    applies inside each program; separate concern from
    program-to-program.
  - `runtime-and-io.md` is the source of truth for the executor
    + plugin model. The coordinator described here is a strict
    superset of today's executor; the existing single-program
    flow is the N=1 case.

---

## Open Questions

These would need to be resolved before building:

  - **World schema declaration.** Where does `World` live? In a
    shared `stdlib/world.ev` that every program imports? In the
    coordinator's own program? Different games have different
    world shapes — should it be per-project?
  - **Conflict resolution for parallel writes.** If two parallel
    programs write to the same `world.foo`, what wins? Options:
    last-writer (depends on schedule), explicit merge rule
    (programmer writes a `World` claim that combines), or
    namespace-by-author (`world.program_A.foo` vs
    `world.program_B.foo`).
  - **Hot reload semantics.** If `level_03.ev` is edited while
    running, when does the change take effect? On next swap?
    Mid-step (force a rebuild)? File watcher + signature bump?
  - **Initial-state format.** JSON is the obvious choice for
    deserializing into `given` + `world.*`. But Evident already
    has `assert ... = ⟨…⟩` syntax for ground facts; could the
    initial state just be an Evident file that gets `import`-ed
    by the first program? That'd keep the format unified.
  - **Halt semantics.** `next_program = "halt"` is one option;
    `next_program = ""` plus an explicit `should_exit ∈ Bool`
    is another. The first is simpler; the second is more readable.
  - **Plugin lifetime across swaps.** SDL window persists?
    Audio device persists? Stdin reader persists? Each plugin
    needs to declare its lifetime relative to programs.

---

## Implementation Path (When We're Ready)

This is forward-looking — not built. The order I'd build in:

  1. **Cache key generalization.** Extend
     `cache: HashMap<String, (CachedSchema, StructuralSignature)>`
     to `cache: HashMap<(ProgramId, StructuralSignature), CachedSchema>`.
     Single program is N=1; semantics unchanged. ~50 lines.
  2. **`next_program` recognition.** Executor reads the binding
     each step; if changed, drop the cache for the previous
     program, load the new program from disk, build a new cache
     against the carried world. ~100 lines plus tests.
  3. **World state extraction.** On swap, walk bindings under
     `world.*`, repackage as `given` for the next program. ~30
     lines. Define the convention in stdlib.
  4. **Program LRU.** Keep N most-recent programs cached.
     ~50 lines.
  5. **Initial-state loader.** `--initial-state file.json` flag
     populates first-frame `given`. ~70 lines.
  6. **Parallel scheduling.** Once swap works serially, add a
     thread pool. Programs declared as "parallel" (in coordinator)
     run on separate threads, sync at frame boundary. Bigger;
     defer until the serial story is solid.

Each step delivers value on its own. Steps 1–5 give you a clean
single-active-program-at-a-time architecture; step 6 unlocks the
multi-agent / UI-split / speculative-load patterns.

---

## Why This Matters

A 52-level game is the obvious motivating case, but the bigger
implication is that **Evident can scale to programs the size of
real systems** (game engines, simulators, IDEs, compilers) without
shoving everything into one Z3 model and hoping. The constraint
solver gets to stay simple — it solves one focused problem at a
time. The coordinator handles composition, lifecycle, and
parallelism using conventional infrastructure (file system, thread
pool, in-memory LRU) without polluting the language surface.

This is the architectural escape hatch. The constraint paradigm
remains pure within each program; the world above is normal
software, with normal scaling tools available.
