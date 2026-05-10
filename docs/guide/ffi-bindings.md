# Writing FFI bindings

How to wrap a C library so Evident programs can call it. Read
`docs/guide/effect-state-machines.md` first if you haven't —
this guide assumes you understand effect dispatch, state machines,
and the issue/await pattern.

> **Repo convention** (see CLAUDE.md): for files we author
> under this repo's `programs/` tree (especially `programs/demos/`
> which doubles as our integration-test set), raw FFI
> primitives — `LibCall`, `FFICall`, `FFIOpen`, `FFILookup` —
> are forbidden. Those files reach C code by either:
>
>   * calling **named claims** from stdlib (`sdl_pump_events`,
>     `gl_clear`, `shell_run_only`, …), or
>   * declaring **FTI typed resources** (`win ∈ SDL_Window (...)`)
>     and letting the bridge own the lifecycle.
>
> Raw FFI lives in `stdlib/` and in Rust-side FTI bridges
> (`runtime-rust/src/event_sources.rs`, `runtime-rust/src/fti.rs`).
> If a demo needs a C function that no stdlib helper covers,
> the fix is to add the helper to stdlib first, then call it
> from the demo. Hardcoded dylib paths like
> `"/opt/homebrew/lib/libSDL2.dylib"` in a demo file are a
> code-review blocker.
>
> This guide is for the stdlib author writing those wrappers.
> (Outside this repo's test set — your own application — there
> is no language-level restriction; you can call `LibCall`
> directly if it's the right tool.)

## The two FFI styles

Evident exposes two effect-level FFI primitives:

| Effect | When to use |
|---|---|
| `LibCall(lib, sym, sig, args)` | **Default.** Cached lib + sym resolution. One effect per C call. |
| `FFIOpen(path)` + `FFILookup(lib, sym)` + `FFICall(sym, sig, args)` | Manual chain. Use when you need explicit handle lifetimes (closing/reopening the same lib) or when you want to inspect the lib/sym handles in your state machine. |

**Use `LibCall` unless you have a specific reason not to.** It
amortizes `dlopen`/`dlsym` to once across the program; subsequent
calls to the same library symbol pay only the libffi-call cost.

## Type signature strings

Both `FFICall` and `LibCall` take a signature like `"i(s)"` —
return type, paren, arg types, paren. One character per type:

| Code | Evident type | C ABI |
|---|---|---|
| `i` | `Int`         | `int64_t` (or `int` for return; libffi widens) |
| `b` | `Bool`        | `int` (0 or 1) |
| `s` | `String`      | `const char*` (UTF-8, null-terminated) |
| `d` | `Real`        | `double` |
| `p` | `Int` (handle) | `void*` |
| `v` | (return only) | `void` |

Examples:
- `"i()"` — no args, returns `Int`. `getpid` is `i()`.
- `"i(s)"` — one String arg, returns Int. `system(cmd)` is `i(s)`.
- `"v(p)"` — one Handle arg, returns nothing. `SDL_DestroyWindow(w)`.
- `"p(siiiii)"` — String + 5 Ints, returns Handle. `SDL_CreateWindow`.

## Arg lists

Args are a `Seq(FFIArg)` (linked-list of tagged values):

```evident
ArgCons(ArgInt(42),
ArgCons(ArgStr("hello"),
ArgCons(ArgHandle(window),
ArgCons(ArgBool(true),
ArgCons(ArgReal(3.14), ArgNil)))))
```

Variants:
- `ArgInt(Int)` — for `i` slots
- `ArgBool(Bool)` — for `b` slots
- `ArgStr(String)` — for `s` slots
- `ArgReal(Real)` — for `d` slots
- `ArgHandle(Int)` — for `p` slots (the runtime resolves the
  handle to its underlying pointer)

Mismatch between sig codes and arg variants → `ErrorResult` at
dispatch time (no segfault).

## A minimal LibCall example

```evident
import "stdlib/runtime.ev"

enum State =
    Init
    Done(Int)

claim main
    state, state_next ∈ State
    last_results ∈ ResultList
    effects ∈ EffectList

    int_out ∈ Int
    int_out = match last_results
        ResCons(r, _) ⇒ match r
            IntResult(n) ⇒ n
            _            ⇒ -1
        _              ⇒ -1

    state_next = match state
        Init     ⇒ Done(int_out)
        Done(c)  ⇒ Done(c)

    effects = match state
        Init     ⇒ EffCons(LibCall("/usr/lib/libSystem.dylib",
                            "getpid", "i()", ArgNil), EffNil)
        Done(_)  ⇒ EffNil
```

After running: state ends up `Done(your_pid)`.

## Library paths

`dlopen` on macOS searches a fixed set of directories that
**doesn't include `/opt/homebrew/lib`** by default. If you bare-name
a Homebrew library it'll silently return `Error("dlopen failed")`.

Recommendations:
- macOS system libs: `"/usr/lib/libSystem.dylib"` (libc, also
  `say`/`system`).
- macOS Homebrew libs: `"/opt/homebrew/lib/libSDL2.dylib"`.
- Linux libs: `"libSDL2-2.0.so.0"` (bare name; ld searches
  `LD_LIBRARY_PATH` plus standard paths).

Hardcode absolute paths until we have a per-platform path-lookup
helper. See `stdlib/sdl/window.ev` for the established pattern.

## Writing a wrapper library

A library file lives under `stdlib/`. Conventions:

1. **Import `stdlib/runtime.ev`** for the Effect / Result / FFIArg
   types.
2. **Effect-builder claims** that take inputs + an output Effect.
   Use `out` (not `effect`/`effects`) for the output param so it
   doesn't shadow main-claim variables on names-match composition.
3. **One claim per C function** is fine; collapse if call signatures
   are identical (same lib + sig pattern).

Example — `stdlib/shell.ev`:

```evident
import "stdlib/runtime.ev"

claim shell_run(cmd ∈ String, out ∈ Effect)
    out = LibCall("/usr/lib/libSystem.dylib", "system", "i(s)",
                  ArgCons(ArgStr(cmd), ArgNil))

claim shell_run_only(cmd ∈ String, out ∈ EffectList)
    out = EffCons(LibCall("/usr/lib/libSystem.dylib", "system", "i(s)",
                  ArgCons(ArgStr(cmd), ArgNil)), EffNil)
```

A user program then writes:

```evident
import "stdlib/shell.ev"

claim main
    ...
    cmd ∈ String
    cmd = "say hello"
    say_eff ∈ EffectList
    shell_run_only (out ↦ say_eff)         -- name-renames out → say_eff

    effects = match state
        Init    ⇒ say_eff
        Done(_) ⇒ EffNil
```

## Capturing FFI return values

A C function's return value lands in `last_results` at the **next**
step (see `effect-state-machines.md` § "Issue → Await pattern").
The `Result` variants:

- `IntResult(Int)` — for `i` and `b` returns
- `StringResult(String)` — for `s` returns (runtime copies the
  C string into Evident-owned memory)
- `RealResult(Real)` — for `d` returns
- `HandleResult(Int)` — for `p` returns (opaque pointer wrapped as
  a Handle ID)
- `BoolResult(Bool)` — never returned by ffi_call directly; reserved
  for future use
- `NoResult` — for `v` returns
- `ErrorResult(String)` — dispatch failure (bad sig, missing handle,
  signature parse error, etc.)

Extract via the two-level match pattern:

```evident
window ∈ Int
window = match last_results
    ResCons(r, _) ⇒ match r
        HandleResult(h) ⇒ h
        _               ⇒ 0
    _              ⇒ 0
```

## Handle lifetimes

LibCall keeps the library + symbol handles cached for the lifetime
of the `DispatchContext` (one program run). You don't need to
`CloseHandle` library handles you got via LibCall — they live until
program exit.

If you used the raw `FFIOpen` + `FFILookup` chain instead, you OWN
those handles and can `CloseHandle(h)` to free them. Closing a
library handle invalidates its symbols (subsequent calls through
those symbols will likely segfault).

C-returned pointers (e.g. `SDL_Window*`) are different — the C
library owns them. Call the appropriate teardown function (e.g.
`SDL_DestroyWindow`) to free; do **not** `CloseHandle` them
directly. The runtime tracks them as opaque IDs but doesn't know
how to free them.

## Argument marshalling pitfalls

- **String null bytes**: `CString::new` rejects strings with embedded
  null bytes. Sanitize first if your data might contain them, or
  the LibCall returns `ErrorResult`.
- **`Handle(0)` is null**: passing `ArgHandle(0)` sends a null
  pointer to the C function. Many C functions crash on null args.
  If you see SIGBUS/SIGSEGV in a LibCall, check whether you're
  accidentally sending Handle(0) (almost always means an earlier
  result-capture failed and the default `_ ⇒ 0` arm fired).
- **Width mismatches**: `i` is i64, but most C `int` is 32-bit. The
  ABI widens for register passing; should be safe in practice. If a
  C function takes `unsigned`, pass a positive Int.
- **Floats**: `Real` → `double`. There's no `f` (float32) code yet.

## Debugging recipe

When an FFI call misbehaves:

1. `EVIDENT_FFI_TRACE=1 evident effect-run program.ev` — see every
   effect input + result.
2. Look for `Error("dlopen ...")` (lib not found at that path).
3. Look for `Handle(0)` in args — means an earlier capture failed.
4. Look for "signature mismatch" / "unknown handle" — argument or
   handle-lifetime issue.
5. If the program runs but the C side seems silent, check whether
   the call needs a follow-up (SDL needs `SDL_PumpEvents` for
   windows to render; audio needs `SDL_PauseAudioDevice(dev, 0)`
   to start playback; etc.).
6. Compare signatures with the C header — many SDL functions take
   `Uint32` not `int`, but `i` (i64) widens correctly.

## Effects vs LibCall caching

`LibCall` is *cached per `DispatchContext`*. The cache lasts for
the run of the program. Repeated calls to the same `(lib, sym)`
do one libffi call each, no re-resolution.

Why this matters in practice:

| Pattern | Cost per call (after first) |
|---|---|
| Raw `FFIOpen` → `FFILookup` → `FFICall` chain | 3 effects, 3 dispatch steps |
| `LibCall` (cached) | 1 effect, 1 dispatch step |

For animation loops at 60fps calling SDL_PumpEvents+SDL_RenderClear
+SDL_RenderFillRect+SDL_RenderPresent, that's the difference between
240 effects/frame and 4 effects/frame.

## When you'd skip LibCall

The raw chain is useful when:

- You need **lifetime control** — explicitly close a library handle
  and reopen it elsewhere.
- You want to **inspect handles in your state machine** for
  debugging or trace assertions.
- You're testing the FFI primitives themselves.

Otherwise, default to `LibCall`.
