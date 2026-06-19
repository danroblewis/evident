# FFI / OS layer evolution

## Purpose

The Rust runtime has reached the point where most of what a user demo
actually needs is already in place. Recent migrations (SDL bridges
out of Rust, declarative install, Hostname via Evident) have shrunk
the runtime substantially. What remains are a handful of gaps where
features almost work but force either a one-off Rust addition or an
awkward workaround.

This doc is the punch list for closing those gaps, ordered by
leverage. Each item is described as one of:

  * **Runtime primitive** — a new Effect or FfiArg variant in Rust.
    Reserved for things genuinely inexpressible in current Evident.
  * **Evident package** — implementable today, lives in
    `packages/` or `stdlib/`, calls existing primitives.

The line we hold: **prefer Evident over Rust**. A new runtime
primitive must justify why the existing FFI primitives + LibCall +
arithmetic can't reach the same outcome.

## Existing primitives (recap)

Effects:
  * `FFIOpen(path) → Handle`, `FFILookup(lib, sym) → Handle`,
    `FFICall(sym, sig, args) → Result`, `CloseHandle(h)`.
  * `LibCall(path, sym, sig, args) → Result` — cached one-shot.
  * `ReadByte(handle, offset) → IntResult(byte)` — single-byte
    pointer deref. Landed for SDL keyboard input.

FFI arg variants:
  * Values: `ArgInt`, `ArgBool`, `ArgStr`, `ArgReal`, `ArgHandle`.
  * Buffers we write: `ArgIntOut` (4-byte out-int),
    `ArgI32Buf` (homogeneous i32 array), `ArgPackedBuf` (mixed-width
    packed struct).
  * Composition: `ArgStrArr` (string array), `ArgPriorResult(N)`
    (reference earlier in-Seq result).

Type codes in signatures: `i`/`b`/`s`/`d`/`f`/`p`/`v`.

OS-level effects: `Print`, `Println`, `ReadLine`, `Time`, `Exit`,
`ParseInt`, `ParseReal`, `IntToStr`, `RealToStr`, `ShellRun`.

Infrastructure: `HandleRegistry` (per-`DispatchContext`, tracks
returned pointers + optional drop fns), declarative install (the
`run_declarative_install` bridge in `runtime/src/trampoline.rs`)
for FTI bridges.

## Tier 1 — Memory read/write primitives

**Status: runtime primitives.** Reading a byte at an offset from a
returned pointer is not expressible by calling a C function (no
generic C function does "deref pointer at offset"). We already
added `ReadByte`; round out the set.

### Reads

```
ReadByte (Int, Int) → IntResult(0..255)      -- already exists
ReadI16  (Int, Int) → IntResult              -- signed 16-bit
ReadI32  (Int, Int) → IntResult              -- signed 32-bit
ReadI64  (Int, Int) → IntResult              -- signed 64-bit
ReadF32  (Int, Int) → RealResult
ReadF64  (Int, Int) → RealResult
ReadStr  (Int, Int) → StringResult           -- null-terminated, UTF-8
```

Each takes `(handle, byte_offset)`. The runtime does
`lookup(handle) + offset` once at dispatch time; Evident never sees
the raw pointer. Reads use unaligned access so they work at any
offset (struct fields aren't always aligned).

### Writes

```
WriteByte (Int, Int, Int)     → NoResult     -- byte ∈ 0..255 clamped
WriteI16  (Int, Int, Int)     → NoResult
WriteI32  (Int, Int, Int)     → NoResult
WriteI64  (Int, Int, Int)     → NoResult
WriteF32  (Int, Int, Real)    → NoResult
WriteF64  (Int, Int, Real)    → NoResult
WriteStr  (Int, Int, String)  → NoResult     -- writes bytes + null terminator
```

The handle must point to writable memory (allocated by Evident via
`Malloc` or by a C function the user knows is OK to write to).
Writing into a const-marked pointer (e.g., the buffer returned by
`SDL_GetKeyboardState`) is undefined behavior the same way it
would be from C.

### Estimated cost

Per primitive: ~12 lines of Rust (one `Effect` variant + one
dispatch arm + one decode arm + one stdlib enum line). Total
package: ~150 lines added, no architectural changes.

## Tier 2 — Allocation

**Status: runtime primitives.** Every pointer in the registry
today comes from an FFI function's return value. There's no way
to create a buffer Evident owns. Needed wherever a C function
writes more than `ArgIntOut`'s 4 bytes — `SDL_PollEvent` writes a
56-byte event; `getline` writes a heap-allocated line; `read(fd)`
needs a destination buffer.

```
Malloc (Int) → IntResult(handle)
  -- allocates `size` bytes (libc malloc), zeroed.
  -- result is a new HandleRegistry entry with `free` as drop fn.
Free   (Int) → NoResult
  -- alias for CloseHandle; included for symmetry/discoverability.
```

The handle's drop fn calls `libc::free`, so cleanup is automatic
at process exit OR on explicit `Free`/`CloseHandle`. Memory leaks
become Evident's responsibility, same as C — the runtime won't
help.

### Why not just `LibCall(libc, "malloc", "p(i)", ⟨ArgInt(size)⟩)`?

That works for the allocation, but the returned pointer is bare —
the HandleRegistry doesn't know to free it on close. A native
`Malloc` effect registers with the right drop fn so the leak is
bounded by program lifetime, not by user diligence.

## Tier 3 — OS coverage (mostly Evident, some Effects)

**Status: Evident packages, except where bypassing dylib paths
matters.** Most of what's missing here is just LibCall to libc /
libsystem; the question is whether to wrap each in an Effect or
let the package handle it.

### Filesystem

Implement in Evident as `packages/posix/file.ev`:

```
external type File
    path ∈ String
    mode ∈ String
    fd   ∈ Int
    install ∈ Seq(InstallStep) = ⟨
        Bind("fd", LibCall("libc", "open", "i(si)",
                            ⟨ArgStr(path), ArgInt(...mode_flags...)⟩))
    ⟩

    subclaim read_bytes(buf ∈ Int, n ∈ Int)
        out = LibCall("libc", "read", "i(ipi)",
                      ⟨ArgInt(fd), ArgHandle(buf), ArgInt(n)⟩)

    subclaim close
        out = LibCall("libc", "close", "i(i)", ⟨ArgInt(fd)⟩)
```

Plus convenience subclaims wrapping pwrite, lseek, fstat, etc.
The runtime gets nothing.

### Time

`Time` exists (wall-clock ms). Add `MonotonicTime` as an
Effect for high-resolution monotonic time (the only common
case where wall-clock isn't right is benchmarking and rate-
limiting — both want a clock that doesn't jump under NTP).

```
MonotonicTime → IntResult(nanoseconds_since_arbitrary_epoch)
```

Why an Effect rather than `LibCall(libc, "clock_gettime", …)`:
`clock_gettime` writes into a `struct timespec` (16 bytes), which
needs `Malloc` + two `ReadI64` calls per query. Native Effect is
one call.

### Sleep

`Sleep(ms) → NoResult`. Trivial native Effect, generalizes
`sdl_delay` to non-SDL programs. Could also be an Evident package
calling `LibCall(libc, "usleep", "v(i)", ⟨ArgInt(ms * 1000)⟩)` —
either works.

### Environment

`packages/posix/env.ev`:

```
external claim getenv(name ∈ String, out ∈ Effect)
    out = LibCall("libc", "getenv", "s(s)", ⟨ArgStr(name)⟩)
```

Returns empty string for missing keys (libc returns NULL; the
FFI marshal converts NULL → empty for `s` returns).

### Random

`packages/posix/random.ev`:

```
external claim random_bytes(n ∈ Int, buf ∈ Int, out ∈ Effect)
    out = LibCall("libc", "getentropy", "i(pi)",
                  ⟨ArgHandle(buf), ArgInt(n)⟩)
```

Caller allocates `buf` via `Malloc(n)`, then `ReadByte` to inspect.

## Tier 4 — Callbacks

**Status: runtime primitive — separate arc.** This is the single
biggest expressiveness multiplier we don't have. Many real C
libraries expect you to register a callback the library invokes
later (timer callbacks, GUI event handlers, completion handlers).
Without it, Evident can only call OUT to C, never be called BACK.

### Shape

```
RegisterCallback(claim_name ∈ String, sig ∈ String) → IntResult(handle)
```

The runtime uses libffi's closure API to build a C-callable
function pointer. When C calls it, the trampoline marshals the
incoming args back into Evident's Result types and dispatches the
named claim, marshalling the claim's return value back out.

### Hard parts

1. **Threading.** The callback may fire on a different thread
   than the scheduler — most GUI libraries route events on the
   main thread but timers may not. Either we restrict to
   main-thread-only callbacks (refuse to set up if the library
   doesn't guarantee that), or we add a thread-safe queue that
   collects callback invocations for the scheduler to drain on
   its next tick. The latter is more flexible but adds latency
   and locking.

2. **Effects-from-callbacks.** If the callback's body emits
   Effects, when do they dispatch? If we dispatch synchronously
   inside the trampoline, we're running effect dispatch
   re-entrantly during another effect dispatch — likely
   undefined. If we queue effects until the scheduler's next
   tick, the callback can't return values that depend on its
   own effects.

3. **Lifetimes.** A callback registered for the program's
   lifetime is fine. A library that registers a callback then
   forgets to deregister it before the closure is dropped =
   crash on next invocation. Handle this via the registry's
   drop function (un-registering on `CloseHandle`) but the user
   has to thread the close call.

### Reasonable v1

Restrict to: main-thread only, callback body may NOT emit
Effects, return value must be a primitive (no Records, no
Seqs). That covers SDL timer callbacks, simple completion
handlers, comparator functions passed to `qsort`. Expand the
restrictions as use cases demand.

Estimated cost: ~400 lines of Rust (libffi closure setup,
marshal both directions, thread guards), plus a meaningful
test bench (memory safety stakes are real).

## Non-goals

* **Variadic functions.** `printf` and friends. Adds significant
  marshal complexity; users can format strings in Evident and
  pass the result to `puts`. Revisit only if a library we want
  insists on it.

* **Long-running threads from Evident.** The runtime owns the
  run-loop thread; Evident-spawned compute threads would need
  their own coordination model.

* **Inter-process primitives.** Pipes, shared memory, sockets.
  Network-level concerns are big enough to deserve their own
  design doc when the use case arrives.

* **Direct syscalls bypassing libc.** Adds platform-specific
  scaffolding for marginal benefit. `LibCall("libc", …)` covers
  POSIX uniformly.

## Implementation order

Suggested sequence, each shippable independently. Status as
of the recent FFI-evolution push:

1. ✅ **Memory reads** (`ReadI16`/`ReadI32`/`ReadI64`/`ReadF32`/
   `ReadF64`/`ReadStr`) — shipped in commit 12f25aa.

2. ✅ **Memory writes** (`WriteByte`/`WriteI16`/... / `WriteStr`)
   — shipped in 4fcf554.

3. ✅ **Allocation** (`Malloc`) — shipped in 40393f1. `CloseHandle`
   already serves as Free.

4. ✅ **`packages/posix/`** — sleep/env/random/file shipped in
   cf59197. Zero Rust changes; proves the primitive set is
   complete enough for POSIX bindings to live entirely in Evident.

5. ✅ **`MonotonicTime`** — shipped in b26e158.

6. 🟡 **Callbacks** — Effect surface defined (commit pending);
   dispatch returns Error pending implementation. Real
   implementation requires:
     * libffi closure setup (the `libffi` crate has a Closure API).
     * Thread-safety: C-side may call from any thread. Decide
       sync-on-main vs async-via-mpsc-to-scheduler. mpsc is more
       general but adds latency and changes the synchronous-
       return-value semantics.
     * Effects-from-callback: probably forbid for v1.
     * Lifetimes: closure's drop fn must un-register before
       dropping; the user must thread the close call.
   The Effect's surface lets future user code be forward-
   compatible — `RegisterCallback(claim_name, sig)` is the call
   shape; only the dispatcher implementation changes.

## Known gap surfaced by Phase 4

The Read/Write/Malloc/MonotonicTime effects take literal handle
Ints, not EffectFfiArg. So they can't appear inside a single
Seq with a just-Malloc'd handle threaded via ArgPriorResult:
within-Seq buffer-then-fill patterns require the handle to be
captured across ticks (via the Bind path used by declarative
install) or via the world snapshot. Lifting this means making
Read/Write/Malloc accept `ArgPriorResult` for the handle slot —
a smaller targeted change to the Effect enum + dispatch.

## After all phases

After Phase 4 we proved the primitive set is complete enough
for POSIX bindings to live in Evident. After Phase 6 the same
will be true for callback-heavy APIs (GUI event loops, async
completions). The runtime stays small (~10K LOC); platform-
specific code lives entirely in `packages/` and `stdlib/`.
