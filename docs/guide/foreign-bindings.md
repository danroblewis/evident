# Foreign bindings — FTI and FFI

How to make C resources usable from Evident, and why the language
treats foreign resources as typed state machines instead of as
function namespaces.

> **Thesis.** A file handle isn't an `int`. A window isn't a
> pointer. `open` / `read` / `write` / `close` aren't independent
> functions — they're operations on a stateful resource whose
> valid sequence the runtime should own. Evident's
> **Foreign Type Interface (FTI)** models foreign resources as
> Evident *types*, lets a Rust-side *bridge* materialize them, and
> treats the bridge as a writer FSM in the multi-FSM scheduler.
> Raw `LibCall` (FFI) remains as an escape hatch when no bridge
> exists yet — but for anything long-lived or stateful, prefer
> FTI.

> **Repo convention** (see CLAUDE.md). For files we author under
> `examples/`, raw FFI primitives (`LibCall`, `FFICall`,
> `FFIOpen`, `FFILookup`) and hardcoded library paths like
> `"/opt/homebrew/lib/libSDL2.dylib"` are forbidden. Demos reach
> C code via FTI typed resources or via named claims in
> `stdlib/`. Raw FFI lives in `stdlib/` and in the Rust-side
> bridges at `runtime/src/event_sources/`.

Read `docs/guide/effect-state-machines.md` first if you haven't —
this guide assumes you understand effect dispatch, world fields,
and the issue/await pattern.

## The two FTI patterns

Both patterns share a common shape: the user's program declares a
typed thing; a Rust-side *bridge* takes ownership of the C-side
resource and writes its observable state into world fields the
scheduler already understands. Single-writer-per-field is enforced
at load time, so a user FSM can never clobber a bridge-owned
field.

### Pattern 1 — Plugin-as-writer (reserved world fields)

The user's `World` type declares a reserved field name; the
runtime auto-installs a bridge that writes it. User FSMs subscribe
through normal world read-set inference — no marker types, no
event channels.

```evident
type World
    tick_count       ∈ Int      -- triggers FrameTimer
    stdin_line       ∈ String   -- triggers StdinSource
    signal_received  ∈ Int      -- triggers SigintSource

claim main(world ∈ World, ...)
    -- referencing world.tick_count wakes the FSM whenever the
    -- bridge increments it
```

Auto-installing field names today:

| World field          | Type     | Bridge              | What it writes |
|---|---|---|---|
| `tick_count`         | `Int`    | `FrameTimer`        | Monotonic counter, fires every `EVIDENT_TICK_MS` (default 100) |
| `stdin_line`         | `String` | `StdinSource`       | Most recent line (also writes `stdin_seq: Int`) |
| `signal_received`    | `Int`    | `SigintSource`      | Count of `SIGINT`s received |
| `wall_clock_ms`      | `Int`    | `WallClockSource`   | Monotonic ms since program start |
| `file_changed` (+ `EVIDENT_FILE_WATCH`) | `Int` | `FileWatcher` | Counter incremented on file change |
| `file_line` (+ `EVIDENT_FILE_INPUT`)    | `String` | `FileLineReader` | Most recent line from the watched file |
| `program`            | (Program enum) | `ReflectionSource` | The loaded program AST |

Each bridge lives in `runtime/src/event_sources/<name>.rs`. The
auto-install logic lives in `WORLD_PLUGIN_INSTALLERS` in
`event_sources/mod.rs`.

### Pattern 2 — Typed-parameter resources

When a resource needs per-instance configuration (multiple windows,
multiple timers at different rates), declare it as an FSM
parameter and pin its config at the declaration site:

```evident
claim fast(state, state_next ∈ S, ...)
    t ∈ Timer (interval_ms ↦ 20)
    state_next = (t.tick_count ≥ 5 ? Done : Run)
    effects = ...

claim main(state, state_next ∈ S, ...)
    win ∈ SDL_Window (title  ↦ "Hello",
                      width  ↦ 640,
                      height ↦ 480)
    -- win.handle, win.gl_handle, win.vao are world fields the
    -- bridge writes; main reads them like any world value
```

The runtime sees the typed parameter, looks up the type in the FTI
registry (`runtime/src/fti.rs`), reads the pinned fields, and
starts a per-instance bridge. Output fields are exposed as
`<name>.<field>`.

FTI registry entries today:

| Type name    | Pinned config                              | Output fields                          |
|---|---|---|
| `FrameClock` | (uses `EVIDENT_TICK_MS`)                   | `tick_count: Int`                      |
| `Timer`      | `interval_ms ↦ Int`                        | `tick_count: Int`                      |
| `Hostname`   | —                                          | `name: String` (one-shot)              |
| `SDL_Window` | `title ↦ String`, `width`, `height ↦ Int`  | `handle`, `gl_handle`, `vao: Int`      |
| `GL_Program` | `vertex_src`, `fragment_src ↦ String`      | `handle: Int`                          |

Multiple FSMs declaring the same type get **distinct** instances —
each declaration site spawns its own bridge. Sharing a single
instance across FSMs isn't a v1 feature; coordinate via world.

## Writing a new bridge

A bridge is a Rust struct implementing `EventSource` (defined in
`runtime/src/event_sources/mod.rs`):

```rust
pub trait EventSource: Send {
    fn start(&mut self, tx: Sender<SchedulerEvent>) -> Result<(), String>;
    fn stop(&mut self);
    fn drain_writes(&mut self) -> Vec<(String, Value)> { Vec::new() }
    fn write_fields(&self) -> Vec<String> { Vec::new() }
}
```

`start` kicks off the C-side work (spawn a thread, `dlopen` + init,
register a signal handler). `drain_writes` is called at the start
of each tick — drained writes are applied through the same
disjoint-write check user FSMs go through. `write_fields` declares
the bridge's write-set at load time.

The simplest shipped example is `OneShotShellSource`
(~80 lines, `event_sources/oneshot_shell.rs`): runs a shell command
once, queues a single `String` write, terminates. Read it before
writing your own. `FrameTimer` (~140 lines) is the simplest
periodic bridge.

### Wiring a bridge into a registry

  * **Plugin-as-writer** (auto-install via world field): add an
    `install_world_plugin` fn that checks
    `ctx.has_world_field("your_field", "Int")`, returns
    `Ok(Some(install))` when the trigger fields are present, and
    append the fn to `WORLD_PLUGIN_INSTALLERS` in
    `event_sources/mod.rs`. No other Rust file needs to change.

  * **Typed parameter** (FTI registry): add an
    `install_<your_type>` fn in `runtime/src/fti.rs` that reads
    pinned config from `pins`, starts the bridge, and returns the
    written keys. Append a row to the `INSTALLERS` table. The
    scheduler discovers the new type through that table.

## Stdlib claim wrappers

The other way to reach C code is from Evident itself: a `stdlib/`
claim that emits a `LibCall` effect with the right signature. Use
this when the C call is genuinely one-shot (a math function, a
syscall), or when wrapping a C library whose state machine isn't
worth modeling in Rust yet.

A library file lives under `packages/<library>/` (e.g.
`packages/sdl/`) when it wraps an external C library; pure
language-level helpers stay in `stdlib/`. Conventions:

1. **Import `stdlib/runtime.ev`** for the Effect / Result / FFIArg
   types.
2. **Effect-builder claims** that take inputs + an output Effect.
   Use `out` (not `effect` / `effects`) for the output param so it
   doesn't shadow main-claim variables on names-match composition.
3. **One claim per C function** is fine; collapse if signatures
   are identical.

Example — `stdlib/shell.ev`:

```evident
import "stdlib/runtime.ev"

claim shell_run(cmd ∈ String, out ∈ Effect)
    out = LibCall("/usr/lib/libSystem.dylib", "system", "i(s)",
                  ArgCons(ArgStr(cmd), ArgNil))
```

A user program then writes:

```evident
import "stdlib/shell.ev"

claim main(state, state_next ∈ S, ...)
    cmd ∈ String
    cmd = "say hello"
    say_eff ∈ Effect
    shell_run (out ↦ say_eff)

    effects = match state
        Init    ⇒ ⟨say_eff⟩
        Done(_) ⇒ ⟨⟩
```

## The FFI escape hatch

When no FTI bridge exists and you can't yet write one, the raw
FFI primitives are still available.

### The two FFI styles

| Effect | When to use |
|---|---|
| `LibCall(lib, sym, sig, args)` | **Default.** Cached lib + sym resolution. One effect per C call. |
| `FFIOpen(path)` + `FFILookup(lib, sym)` + `FFICall(sym, sig, args)` | Manual chain. Use when you need explicit handle lifetimes or want to inspect lib/sym handles in your state machine. |

**Default to `LibCall`.** It amortizes `dlopen` / `dlsym` to once
across the program; subsequent calls pay only the libffi-call
cost. For animation at 60fps calling pump+clear+fill+present, the
cached path is 4 effects/frame vs. 240 effects/frame for the raw
chain.

### Type signature strings

Signature like `"i(s)"` — return type, paren, arg types, paren.
One char per type:

| Code | Evident type   | C ABI |
|---|---|---|
| `i` | `Int`         | `int64_t` (or `int` for return; libffi widens) |
| `b` | `Bool`        | `int` (0 or 1) |
| `s` | `String`      | `const char*` (UTF-8, null-terminated) |
| `d` | `Real`        | `double` |
| `p` | `Int` (handle)| `void*` |
| `v` | (return only) | `void` |

Examples:
- `"i()"` — no args, returns Int. `getpid` is `i()`.
- `"i(s)"` — String arg, Int return. `system(cmd)` is `i(s)`.
- `"v(p)"` — Handle arg, void return. `SDL_DestroyWindow(w)`.
- `"p(siiiii)"` — String + 5 Ints, returns Handle.
  `SDL_CreateWindow`.

### Arg lists

Args are a `Seq(FFIArg)` (linked-list of tagged values):

```evident
ArgCons(ArgInt(42),
ArgCons(ArgStr("hello"),
ArgCons(ArgHandle(window),
ArgCons(ArgBool(true),
ArgCons(ArgReal(3.14), ArgNil)))))
```

Variants: `ArgInt`, `ArgBool`, `ArgStr`, `ArgReal`, `ArgHandle`.
A mismatch between sig codes and arg variants → `ErrorResult` at
dispatch time (no segfault).

### Capturing return values

Return values land in `last_results` at the *next* step (see
`effect-state-machines.md` § "Issue → Await pattern"):

- `IntResult(Int)` — for `i` and `b` returns
- `StringResult(String)` — for `s` returns (runtime copies into
  Evident-owned memory)
- `RealResult(Real)` — for `d` returns
- `HandleResult(Int)` — for `p` returns (opaque pointer wrapped
  as a Handle ID)
- `NoResult` — for `v` returns
- `ErrorResult(String)` — dispatch failure (bad sig, missing
  handle, signature parse error)

Extract via the two-level match pattern:

```evident
window ∈ Int
window = match last_results
    ResCons(r, _) ⇒ match r
        HandleResult(h) ⇒ h
        _               ⇒ 0
    _              ⇒ 0
```

### Library paths

`dlopen` on macOS searches a fixed set of directories that
**doesn't include `/opt/homebrew/lib`** by default. Bare-naming a
Homebrew library silently returns `Error("dlopen failed")`.

- macOS system libs: `"/usr/lib/libSystem.dylib"` (libc, `system`,
  `say`)
- macOS Homebrew libs: `"/opt/homebrew/lib/libSDL2.dylib"`
- Linux libs: `"libSDL2-2.0.so.0"` (bare name; ld searches
  `LD_LIBRARY_PATH` + standard paths)

Hardcode absolute paths until we have a per-platform path-lookup
helper. See `packages/sdl/window.ev` for the established pattern.

### Handle lifetimes

`LibCall` caches library + symbol handles for the lifetime of the
`DispatchContext` (one program run). You don't need to
`CloseHandle` them — they live until program exit.

If you used the raw `FFIOpen` + `FFILookup` chain, you **own**
those handles and can `CloseHandle(h)` to free them. Closing a
library handle invalidates its symbols; subsequent calls through
those symbols likely segfault.

C-returned pointers (e.g., `SDL_Window*`) are the C library's
responsibility — call the appropriate teardown function (e.g.,
`SDL_DestroyWindow`). Do **not** `CloseHandle` them.

### Argument marshaling pitfalls

- **String null bytes**: `CString::new` rejects embedded nulls.
  Sanitize first or `LibCall` returns `ErrorResult`.
- **`Handle(0)` is null**: passing `ArgHandle(0)` sends a null
  pointer to C. SIGSEGV in a `LibCall` almost always means an
  earlier capture failed and the default `_ ⇒ 0` arm fired.
- **Width mismatches**: `i` is `i64`; most C `int` is 32-bit. ABI
  widens for register passing; safe in practice.
- **Floats**: `Real` → `double`. No `f` (float32) code yet.

### Debugging recipe

`EVIDENT_FFI_TRACE=1 evident effect-run program.ev` — trace every
effect input and result.

Common failures:
1. `Error("dlopen ...")` — lib not found at that path.
2. `Handle(0)` in args — earlier result-capture failed.
3. "signature mismatch" / "unknown handle" — argument or
   handle-lifetime issue.
4. Program runs but C side seems silent — call needs a follow-up
   (SDL needs `SDL_PumpEvents` for windows to render; audio needs
   `SDL_PauseAudioDevice(dev, 0)` to start playback).

## Known gap — operation-sequence validation

The current FTI gives the runtime *ownership* of foreign resources
(constructor, destructor, single-writer-per-field). It does **not**
yet model a resource's *valid operation sequence*. Today nothing
in the runtime knows that `read(fd)` after `close(fd)` is invalid,
or that `SDL_DestroyWindow(w)` followed by `SDL_GL_SwapWindow(w)`
will segfault. The bridge owns the C pointer so the user can't
free it manually — but the *order* of operations on a live
resource is the user's responsibility.

The thesis-completing v2 work is to declare each foreign type's
lifecycle stages as an Evident enum, tag each operation with its
valid pre-state, and let the solver enforce the sequence at load
time. Tracking: `docs/design/foreign-type-interface.md`.
