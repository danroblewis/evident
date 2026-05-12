# Foreign Type Interface (FTI)

**Status**: v1 shipped (12 bridges live in
`runtime/src/event_sources/`); v2 work in progress (more bridges,
reactive config sync); v3 — operation-sequence validation — is the
open thesis-completing question.

## The thesis: foreign resources are state machines, not function namespaces

A file handle isn't an `int`. A window isn't a pointer.
`open` / `read` / `write` / `close`, `socket` / `bind` / `listen`
/ `accept`, `fopen` / `fread` / `fseek` / `fclose` — none of these
are independent functions. They're operations on stateful
resources, and only certain sequences are valid:

| C surface                                                | Underlying state machine |
|---|---|
| `SDL_Init` → `SDL_CreateWindow` → `SDL_GL_CreateContext` → … | A growing tree of OS/GPU state. The Window handle is a reference into kernel-side state. |
| `socket()` → `bind()` → `listen()` → `accept()`              | A network socket FSM living in the kernel. |
| `fopen` → `fread` → `fseek` → `fclose`                       | A file handle FSM. |
| `pthread_create` → `pthread_mutex_lock` → `pthread_join`     | Thread + mutex state. |

C's function-call shape doesn't surface this state; the programmer
threads handles through their own variables and trusts themselves
to call the matching cleanup in the right order. That's fine in C,
where the programmer is "in" the state machine implicitly. It's a
poor fit for Evident, where every other piece of state IS a typed
value the runtime knows about.

The Foreign Type Interface models foreign resources as **types**
with bridge plugins that materialize them:

```evident
claim main(state, state_next ∈ S, ...)
    win ∈ SDL_Window (title  ↦ "hello",
                      width  ↦ 640,
                      height ↦ 480)
    -- the rest of the app reads win.handle, win.gl_handle, etc.
```

A Rust-side `SdlWindowSource` bridge:
- Sees `win ∈ SDL_Window` → calls `SDL_Init` + `SDL_CreateWindow`
  with the declared title/size.
- Owns the window pointer for the program's lifetime; calls
  `SDL_DestroyWindow` on drop.
- Writes its handles (`win.handle`, `win.gl_handle`, `win.vao`)
  into world fields the user FSM reads like any other world value.

The user FSM never sees a raw pointer. They never call a function
to materialize the window. They declare what they want; the bridge
makes it real and the multi-FSM scheduler treats the bridge as a
writer FSM — same delta-driven scheduling, same single-writer-per-
field rule, same coordination model.

## What's shipped (v1)

### Two declaration patterns

  * **Plugin-as-writer.** The user's `World` type declares a
    reserved field name; the runtime auto-installs a bridge that
    writes it. User FSMs subscribe through normal world read-set
    inference. Examples: `tick_count: Int` → FrameTimer,
    `stdin_line: String` → StdinSource, `signal_received: Int` →
    SigintSource. Registry: `WORLD_PLUGIN_INSTALLERS` in
    `runtime/src/event_sources/mod.rs`.

  * **Typed-parameter.** An FSM declares `name ∈ Type (...)` with
    optional pinned config (`interval_ms ↦ 20`). Each declaration
    site gets a distinct bridge instance. Registry: `INSTALLERS`
    in `runtime/src/fti.rs`. Types currently supported:
    `FrameClock`, `Timer`, `Hostname`, `SDL_Window`, `GL_Program`.

### Lifecycle ownership

Bridges own the C-side resources. The user FSM holds only the
field values the bridge publishes — handles as opaque IDs,
observed state mirrored from C events. On scheduler shutdown,
each bridge's `Drop` impl runs the corresponding teardown
(`SDL_DestroyWindow` + `SDL_Quit`, etc.). The user cannot leak by
forgetting to close — there is no "close" call in their code.

### Coordination with user FSMs

Bridge writes go through the same write-queue → world-update path
as user-FSM writes. The multi-writer disjoint-set check fires at
load time: a user FSM declaring `world_next.tick_count` collides
with FrameTimer's ownership of `tick_count` and the runtime
rejects the program with a clear error.

### Identity / instances

**Distinct-by-default.** Each declaration site spawns its own
bridge instance. Two FSMs declaring `t ∈ Timer (interval_ms ↦ 20)`
and `t ∈ Timer (interval_ms ↦ 100)` get two timers at independent
rates. The original design doc weighed shared-by-default vs.
distinct-by-default; experience has settled on distinct (the OS
reality is usually one resource per declaration, and the user's
mental model — "I declared it, it's mine" — matches). Cross-FSM
sharing requires coordination via world.

## What's not yet shipped (v2 work)

### Lifecycle hooks beyond construct/destruct

Today, the bridge knows when its first declaration appears (start
of program) and when the runtime exits (drop). It does not track
"the FSM declaring this resource halted" — only program-wide
shutdown. Mid-program resource release isn't yet possible.

### Reactive config sync

A user write to `win.size` ought to call `SDL_SetWindowSize`.
Today, bridges write their observed state to world fields but
don't watch for user-side writes to react. For SDL_Window the
workaround is to recreate the window with new pins; for genuinely
mutable resources this is the limiting case.

### Surface coverage

The five FTI types in the registry cover timers, hostname, and the
SDL window / GL program pair. Sockets, files, child processes,
audio devices — all the obvious foreign types — don't yet have
bridges. Each one is a self-contained addition: implement
`EventSource`, add a row to the registry.

## What's missing — the v3 gap

The current FTI gives the runtime *ownership* of foreign
resources. It does **not** model their valid operation sequences.
The runtime knows:

- Who can write each world field (disjoint-set check).
- When to construct (declaration appears) and destruct (scheduler
  shutdown).

It does **not** know:

- That `read(fd)` after `close(fd)` is invalid.
- That `SDL_DestroyWindow(w)` followed by `SDL_GL_SwapWindow(w)`
  segfaults.
- That `bind()` must precede `listen()` must precede `accept()`.

The thesis-completing move is to declare each foreign type's
lifecycle stages as an Evident enum, tag each operation with its
valid pre-state, and let the solver enforce the sequence:

```evident
enum FileLifecycle = Opened | Closed
type FileHandle
    state ∈ FileLifecycle
    -- read:  state = Opened → state' = Opened
    -- close: state = Opened → state' = Closed
    -- (no read transition from Closed)
```

The solver would then reject a program whose effect sequence
implies `state = Closed ∧ Read(handle)` at load time, before any
C code runs. That folds the thing C requires the programmer to
track in their head — "is this handle still open?" — into the
type system Evident already runs.

This is research, not implementation work. The data model is
straightforward; the open question is how to surface the
operation-tagging so it stays ergonomic (no per-call boilerplate)
and so a stdlib author wrapping a new C library has a clear path
from C-header to lifecycle-annotated Evident type.

## v1 → v2 → v3

  * **v1 (today)**: foreign types declared, bridges materialize
    C resources, single-writer enforcement. Operation sequences
    are the user's responsibility.
  * **v2**: per-claim resource ownership tracking; reactive
    config writes; more bridges (sockets, files, audio).
  * **v3**: operation-sequence validation. Solver-enforced
    lifecycles. The thesis fully realized.

## See also

  * [`schema-interface.md`](schema-interface.md) — the unified
    schema model (foreign types are schemas with bridge plugins).
  * [`fsm-subscriptions.md`](fsm-subscriptions.md) — the
    scheduler bridges plug into as writer FSMs.
  * [`../guide/foreign-bindings.md`](../guide/foreign-bindings.md)
    — how-to for writing bridges and using FTI from Evident code.
  * [`ffi-design.md`](ffi-design.md) — `Effect::FFICall` itself.
    FFI remains as the escape hatch when no bridge exists yet.
