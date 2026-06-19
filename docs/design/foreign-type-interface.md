# Foreign Type Interface (FTI)

Status: design direction (no v1 implementation; v2+ work).

## The current state of FFI

Today, Evident calls into C through `Effect::FFICall`:

```evident
sdl_init_eff = LibCall("/opt/.../libSDL2.dylib",
                       "SDL_Init", "i(i)",
                       ArgCons(ArgInt(32), ArgNil))
```

The dispatcher loads the library, looks up the symbol, marshals
arguments, calls the function, and returns a result. Each call is
stateless from the runtime's perspective — opaque integer/handle
values flow back, and the user FSM is responsible for tracking
what those handles mean and what state they refer to.

This works. It's also a poor mental model for stateful resources.

## Why functions are a poor model for state

Look at any non-trivial FFI workload:

| Surface | What's actually being managed |
|---|---|
| `SDL_Init` → `SDL_CreateWindow` → `SDL_GL_CreateContext` → … | A growing tree of OS/GPU state. The Window handle is a reference into kernel-side state. |
| `socket()` → `bind()` → `listen()` → `accept()`              | A network socket FSM living in the kernel. |
| `fopen` → `fread` → `fseek` → `fclose`                       | A file handle FSM. |
| `pthread_create` → `pthread_mutex_lock` → `pthread_join`     | Thread + mutex state. |

In every case there's a real state machine — handles, file
offsets, connection states, allocated buffers. C's function-call
shape doesn't represent that state explicitly; the programmer
threads it through their own variables and trusts themselves to
call the matching cleanup function in the right order.

This is fine in C (where the programmer is "in" the state machine
implicitly). It's a worse fit for Evident, where every other piece
of state IS a typed value the runtime knows about.

## Foreign Type Interface

Instead of borrowing C's function-call shape, model C resources
as **types** with bridge plugins that materialize them:

```evident
type SDL_Window
    title       ∈ String
    size        ∈ IVec2
    position    ∈ IVec2
    fullscreen  ∈ Bool
    -- runtime injects: handle ∈ Int (opaque, plugin-managed)
    --                  focused ∈ Bool (mirrored from SDL events)

claim my_app
    win ∈ SDL_Window
    win.title = "hello"
    win.size  = IVec2(640, 480)
    -- (the rest of the app — uses win.size, win.position, etc.)
```

A Rust-side `SDLPlugin` observes the program's declarations:

  * Sees `win ∈ SDL_Window` → calls `SDL_CreateWindow` to materialize
    the window with the user's title/size.
  * Watches user writes to `win.size` → calls `SDL_SetWindowSize`.
  * Watches SDL events (resize, focus change) → mirrors them back
    into `win.size`, `win.position`, `win.focused` via world writes.
  * When no FSM declares `win` anymore (program is exiting / FSM
    halted) → calls `SDL_DestroyWindow`.

The user FSM never sees a handle. They never call a function. They
declare what they want; the bridge makes it real.

## What this gives us

  * **Resource lifecycle is automatic.** The bridge plugin owns
    `SDL_DestroyWindow`/`fclose`/`close`/etc. — runs when the
    declaration goes out of scope. No leak-by-forgetting-to-call-
    cleanup.
  * **State is observable, not opaque.** `win.size` is a real Int
    pair the user FSM can read at any time. Currently it'd be a
    chain of `SDL_GetWindowSize` calls returning fresh values.
  * **Typed.** `win.title` is a String. `win.size` is `IVec2`. No
    "is this u32 a width or a handle" guesswork.
  * **Composable.** A type can be passed as a parameter, included
    via `..`, embedded as a field of another type — same as any
    Evident type. The bridge follows the value.
  * **State lives in the FSM's world.** The user FSM reads and
    writes the type's fields as ordinary world state; the bridge
    materializes the C-side resource and mirrors its state into
    those fields. The bridge is Rust code the runtime runs around
    each tick — not a separate FSM the user coordinates with.

## What needs to be built

This is a design direction, not a v1 deliverable. Concrete pieces:

### 1. Type-with-bridge declaration

A way to mark a type as foreign-managed. Two options:

  * **Convention**: types whose names match a registered prefix or
    suffix (e.g. `SDL_*`, `*_Handle`) are auto-bridged.
  * **Explicit**: a `bridge_plugin = "..."` annotation on the
    type, or a separate `extern type` keyword.

Explicit is more honest. The type definition gains either a
metadata block or extends syntax to declare its bridge.

### 2. Bridge plugin protocol

A bridge plugin needs to:

  * Be notified when a value of its type is declared (FSM A
    declares `win ∈ SDL_Window`) — to materialize the resource.
  * Be notified when its fields are written (user sets `win.size`)
    — to issue the corresponding C call.
  * Be able to write its fields from outside (user reads
    `win.position` after a resize event) — to mirror state.
  * Be notified when no FSM declares the value — to clean up.

The first three are existing capabilities (the bridge reads and
writes the type's world fields around each tick). The last
(declaration tracking) is new — we'd need the runtime to know
"is this resource still referenced?"

### 3. Identity / instances

Multiple FSMs declaring `win ∈ SDL_Window` — is that ONE window
shared between them, or N separate windows? The Evident model
doesn't have implicit instance identity. Two options:

  * **Shared by default**: all references to a type name resolve
    to the same instance. The bridge materializes one resource.
  * **Distinct by default**: each declaration is a fresh instance.
    The bridge materializes N resources. Sharing requires a named
    reference.

The OS / hardware reality usually wants distinct (one window per
declaration). The user's mental model is probably also distinct
("I declared it, it's mine"). Distinct-by-default plus named
sharing seems right.

This is the "instances of declared things" question — how many
real resources one type name maps to.

### 4. Migration of existing FFI bindings

Current `packages/sdl/`, `packages/gl/`, etc. are written against
`Effect::FFICall`. Migrating to FTI is a per-binding effort. The
two paths can coexist — `Effect::FFICall` doesn't go away. New
bindings can use FTI; old ones stay as-is until rewritten.

## v1 vs v2 vs v3

  * **v1 (today)**: `Effect::FFICall`, opaque handles, the FSM
    encodes the state machine. Works; not pretty. Typed FTI
    resources (`win ∈ SDL_Window (...)`) already ride the
    declarative-install bridge for materialization.
  * **v2**: extend the bridge protocol with field-write callbacks
    and declaration tracking, so live field mirroring and cleanup
    happen without the FSM threading handles.
  * **v3**: full FTI rollout. Most stdlib FFI bindings become
    type declarations. `Effect::FFICall` remains for genuinely
    one-shot calls (math libraries, simple syscalls).

## See also

  * [`schema-interface.md`](schema-interface.md) — the unified
    schema model (this is just FFI as schema).
  * [`ffi-design.md`](ffi-design.md) — current `Effect::FFICall`
    design and tradeoffs.
