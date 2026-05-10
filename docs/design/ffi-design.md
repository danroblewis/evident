# FFI Layer Design

## Purpose

Let Evident programs call any C library or POSIX syscall. Replace the
current plugin architecture (SDL, audio, shader, stdin/stdout each with
~500 lines of dedicated Rust) with one generic primitive that Evident
libraries build on.

The FFI is how the runtime causes side-effects in response to state
changes. The constraint solver produces a `(next_state, effects)` pair
each step; the runtime walks the effect list and, for each effect,
either runs a built-in or dispatches via FFI to a C function.

## What FFI provides

Three primitives in the runtime, exposed to Evident programs through
the effect type:

```
ffi_open(path: String) → Handle           -- dlopen(path, RTLD_NOW)
ffi_lookup(lib: Handle, sym: String) → Handle  -- dlsym(lib, sym)
ffi_call(fn: Handle, sig: String, args: ArgList) → Result
```

`sig` is a compact type signature string: return type followed by `(`
arg types `)`. Same convention Lua's LuaJIT FFI and Python ctypes use.

Type codes:
- `i` — int64 (Evident Int → C long long)
- `b` — bool (Evident Bool → C int 0/1)
- `s` — UTF-8 null-terminated string (Evident String → const char*)
- `d` — double (Evident Real → C double)
- `p` — opaque pointer (Evident Handle → void*)
- `v` — void return only

Examples:
- `i()` — zero args, returns Int. `getpid` is `i()`.
- `i(s)` — one String arg, returns Int. `puts(s)` is `i(s)`.
- `p(sii)` — two Strings + one Int, returns Handle. SDL_CreateRGBSurface might be `p(siiii)`.
- `i(piii)` — Handle + 3 Ints, returns Int.

Anything more exotic (structs, function pointers, callbacks) is not
v1. Most useful libraries can be wrapped using the codes above plus
helper FFI accessors like `sdl_event_type(event_handle) → Int` for
struct-field reads.

## Type marshalling

| Evident → C | C → Evident |
|---|---|
| `Int` → `int64_t` | `int64_t` → `Int` |
| `Bool` → `int` (0 or 1) | `int` (any nonzero) → `Bool true` |
| `String` → `const char*` (null-terminated UTF-8, copy-in) | `const char*` → `String` (copy-out, deferred-strlen) |
| `Real` → `double` | `double` → `Real` |
| `Handle` → `void*` | `void*` → `Handle` |
| Anything else | Reject at signature parse time |

Strings cross as **copies in both directions**. Rust manages the
buffer lifetime. C never sees Rust-owned memory after the call returns.

## Opaque handles

C libraries return raw pointers (`SDL_Window*`, `FILE*`, etc.). These
become Evident `Handle` values: opaque u64 IDs.

The runtime keeps `HashMap<u64, ResourceEntry>`. Each entry stores:
- The raw `*mut c_void` pointer
- An optional cleanup closure (`Box<dyn FnOnce()>`) for explicit Close

When the Evident program issues a `CloseHandle(h)` effect, the runtime
runs the cleanup closure (if any) and removes the entry. There is no
GC; programs manage their own resource lifetimes.

Handles allocated with no cleanup closure (the default) leak when the
runtime exits. Cleanup is opt-in via library code — `stdlib/sdl/` would
register `SDL_DestroyWindow` as cleanup when allocating a window.

## The effect type

Defined in `stdlib/runtime.ev`:

```evident
type Effect =
    | None                                    -- no-op; pads an effect list
    | Print(String)                            -- write to stdout, no newline
    | Println(String)                          -- write to stdout, with newline
    | ReadLine                                 -- read one line from stdin
    | Time                                     -- monotonic time in ms
    | Exit(Int)                                -- exit with status

    | FFIOpen(String)                          -- dlopen
    | FFILookup(Handle, String)                -- dlsym
    | FFICall(Handle, String, ArgList)         -- libffi call
    | CloseHandle(Handle)                      -- free a managed handle

type Result =
    | NoResult
    | IntResult(Int)
    | StringResult(String)
    | BoolResult(Bool)
    | RealResult(Real)
    | HandleResult(Handle)
    | Error(String)
```

The effect list is `Seq(Effect)`; the result list is `Seq(Result)`,
positionally aligned with the effects performed in the previous step.

## Effect dispatch

The step engine, after each Z3 solve:

```
for each effect in next_state.effects:
    result = match effect:
        None         ⇒ NoResult
        Print(s)     ⇒ stdout_write(s); NoResult
        Println(s)   ⇒ stdout_write_line(s); NoResult
        ReadLine     ⇒ StringResult(stdin.read_line())
        Time         ⇒ IntResult(monotonic_ms())
        Exit(n)      ⇒ process_exit(n)
        FFIOpen(p)   ⇒ HandleResult(register(dlopen(p)))
        FFILookup(h, s) ⇒ HandleResult(register(dlsym(h, s)))
        FFICall(h, sig, args) ⇒ marshall_and_call(h, sig, args)
        CloseHandle(h) ⇒ free(h); NoResult
    results.push(result)
```

Each result becomes the next step's `last_results` input. The Evident
program reads results positionally and decides what to do based on
them.

## Safety bounds

- **Signature validation**: signatures are parsed once into a libffi
  `cif` struct at call time. Bad signatures fail with a runtime error
  before any C call runs.
- **Argument count check**: signature arg-count must equal the
  ArgList length. Mismatch is a runtime error.
- **Argument type check**: each arg's Evident type must match the
  signature character. Mismatch is a runtime error.
- **String encoding**: UTF-8 only. Strings with embedded null bytes
  are rejected before the C call.
- **Handle validation**: every Handle arg must be a registered handle.
  Stale or invented handles are rejected.
- **No callbacks**: v1 doesn't support C calling back into Evident.
  Adds substantial complexity; revisit if needed.

The FFI is unsafe at the Rust level (libffi calls into arbitrary C),
but the safety bounds above mean Evident programs cannot trivially
crash the runtime by passing bad arguments. They can still crash it
by, say, calling `kill(getpid(), SIGSEGV)` — that's intentional.

## Built-in effects

Some effects don't go through FFI even though they could:

| Effect | Why built-in |
|---|---|
| `Print`, `Println` | Used during bootstrap; FFI may not be set up. Cheap to implement directly. |
| `ReadLine` | Same. |
| `Time` | Avoids one FFI hop on the hot path of every animation/timer. |
| `Exit` | Has to be runtime-controlled (FFI exit() would skip Rust cleanup). |
| `FFIOpen` / `FFILookup` / `FFICall` / `CloseHandle` | The FFI mechanism itself. |

Anything else (file I/O, time-of-day, network sockets, environment
variables) goes through FFI to libc. `stdlib/posix.ev` will wrap the
common syscalls into idiomatic Evident claims.

## Trace tests under FFI

Trace tests today use `trace_runner.rs` to deterministically drive
plugins. Under the FFI model, trace tests need a way to substitute
real FFI calls with recorded results.

Plan: a trace-test mode flag on the step engine. When enabled:

- `FFIOpen` / `FFILookup` succeed but return a sentinel handle.
- `FFICall` consults a recorded log of `(symbol_name, args) → result`
  pairs supplied by the test. If the call matches the next entry, it
  returns the recorded result. If it doesn't, the test fails.

Recording mode: the runtime executes real FFI calls and writes the
log alongside the test's input script. Tests then become reproducible
without needing the actual library at test time.

This keeps trace_runner.rs at ~250 lines (was 533): just the
record/replay shim plus existing assertion logic.

## What `stdlib/posix.ev` looks like

A sketch — actual file lands with the FFI implementation:

```evident
import "stdlib/runtime.ev"

-- Lazy-initialized libc handle. The first call resolves; subsequent
-- calls reuse. Caller's first claim should hold libc and pass it down.
claim libc_handle(h ∈ Handle)
    h = ffi_open("libc.so.6")        -- Linux; macOS uses "libSystem.dylib"

claim getpid(h ∈ Handle, pid ∈ Int)
    fn ∈ Handle
    fn = ffi_lookup(h, "getpid")
    pid = ffi_call(fn, "i()", [])

claim open_file(h ∈ Handle, path ∈ String, flags ∈ Int, fd ∈ Int)
    fn ∈ Handle
    fn = ffi_lookup(h, "open")
    fd = ffi_call(fn, "i(si)", [path, flags])

claim close_fd(h ∈ Handle, fd ∈ Int, ok ∈ Bool)
    fn ∈ Handle
    fn = ffi_lookup(h, "close")
    rc ∈ Int = ffi_call(fn, "i(i)", [fd])
    ok = rc = 0
```

Libraries like `stdlib/sdl/` import `stdlib/posix.ev` for shared
plumbing, then add their own FFI wrappers. The pattern compounds —
every library written this way reduces what the runtime needs to
know.

## Implementation plan

| Step | Effort | What lands |
|---|---|---|
| 1. Add deps: `libloading`, `libffi` (or `dlopen2` + manual ffi) | 30m | `Cargo.toml`. |
| 2. Write `runtime/src/ffi.rs` with `LoadLibrary`/`LoadSymbol`/`Call` primitives | ~400 lines | Standalone, unit-tested. |
| 3. Add `Effect` and `Result` types in AST (or use stdlib enums via existing infrastructure) | 1h | Just type definitions — no integration yet. |
| 4. Stub `Effect` dispatch in executor: handle Print/Println/Exit only | ~100 lines | Validates the effect-loop shape without FFI. |
| 5. Wire FFI effects (`FFIOpen`, `FFILookup`, `FFICall`, `CloseHandle`) into the dispatcher | ~150 lines | First end-to-end FFI call from Evident. |
| 6. Write `examples/ffi_getpid.ev` | ~30 lines Evident | Prove the loop works. |
| 7. Write `stdlib/posix.ev` skeleton | ~100 lines Evident | Foundation for everything else. |
| 8. Trace-test record/replay shim | ~150 lines | So FFI tests are reproducible. |

After this lands: the SDL/audio/shader migrations become straight
library work, no runtime changes needed.

## What to read next

- `docs/design/minimal-runtime.md` — the broader picture this fits into.
- The first FFI commit will include the demo program and walk through
  one full round trip: Evident effect → libffi call → result back into
  Evident.
