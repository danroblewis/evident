# Event sources as one generic awaiter + Evident source definitions

## Thesis

Every async event source in the runtime today is the same shape:

```rust
thread::spawn(move || loop {
    <block on one I/O thing>;        // nanosleep / read(2) / clock_gettime / stat
    tx.send(SchedulerEvent::Tick);   // wake the scheduler
    queue.push((world_field, value)) // publish a world write
});
```

Six files (`frame_timer.rs`, `stdin.rs`, `file_line_reader.rs`,
`file_watcher.rs`, `wall_clock.rs`, `sigint.rs`) are variations on
that one loop. Each carries its own `EventSource` impl, its own
`WriteQueue`, its own `stop`/`Drop` plumbing — ~650 lines of nearly
identical Rust.

That shape splits cleanly into two halves:

- **FFI-able (could be Evident).** The I/O syscall itself —
  `nanosleep` (timer), `read(2)` (stdin / file), `clock_gettime`
  (wall_clock), `stat`/vnode-watch (file_watcher) — plus the *parse*
  (split a line, decode bytes) and the *decision* of which world field
  to write. All of this is `LibCall` + `Read*` + arithmetic + string
  ops, every one of which Evident already has.
- **Irreducibly Rust.** The background thread that blocks on I/O
  *concurrently with* a running solve, and the channel that wakes the
  scheduler. A synchronous constraint tick cannot block on I/O without
  freezing the entire scheduler (see § 1), so *something* off the
  scheduler thread has to do the waiting.

The design this doc develops: collapse the irreducible half into **one
generic Rust awaiter** — a single kqueue/epoll reactor over a
*registered set* of `{fd-readiness, timer}` descriptors that wakes the
scheduler with "descriptor X fired" — and move the FFI-able half into
**Evident source definitions** declared through the existing FTI
install procedure. On wake, the source FSM does the non-blocking FFI
read, parses, and writes a world field through the ordinary
FSM → world-write path.

This collapses five of the six sources into Evident (timer, stdin,
file-reader, file-watcher, wall_clock), unifies the two near-identical
line readers (stdin = fd 0) into one, and leaves exactly one source in
Rust — sigint — for reasons § 4 makes precise.

This is the natural conclusion of two things the runtime already
believes: "**sources are FSMs too**" (CLAUDE.md — today they're FSMs
written in Rust; this makes them FSMs written in Evident) and "**prefer
Evident over Rust**" (`docs/design/ffi-design.md` — a Rust primitive
must justify why FFI + arithmetic can't reach the outcome). The only
thing FFI + arithmetic genuinely *cannot* express is "block off-thread
until an fd is readable, then wake me." That — and nothing else — is
what the awaiter is.

---

## § 1 — Anatomy + the FFI-able / irreducible split

### The trait, as it stands

`event_sources/mod.rs` defines the contract every source implements:

```rust
pub trait EventSource: Send {
    fn start(&mut self, tx: Sender<SchedulerEvent>) -> Result<(), String>;
    fn stop(&mut self);
    fn drain_writes(&mut self) -> Vec<(String, Value)> { Vec::new() }
    fn write_fields(&self) -> Vec<String> { Vec::new() }
}
```

`start(tx)` spawns the thread. The thread blocks on its I/O, sends a
`SchedulerEvent::Tick { name }` on each event, and pushes
`(field, value)` writes onto a shared `WriteQueue`. The scheduler
drains that queue between ticks (`scheduler.rs` — STATE writes applied
all-at-once, EVENT writes one-per-tick so each is observable) and uses
the wake to decide which FSMs tick next. When all senders drop, the
scheduler's `rx.recv()` returns `Err` and the program halts cleanly.

### Why a synchronous solve can't do the waiting itself

The multi-FSM scheduler is a single thread that runs Z3 solves
synchronously. A tick is: solve each ready FSM, dispatch its effects,
apply world writes, decide who wakes next. If an FSM body emitted a
`read(fd)` `LibCall` that blocked, the whole scheduler thread would
block *inside effect dispatch* — no other FSM could tick, no timer
could fire, no signal could land. "Block until input arrives" is not a
constraint; it has no satisfying assignment to compute, it's a
suspension of computation. The solver has nothing to do while waiting,
and it holds the only thread.

So the wait has to happen somewhere else, and that somewhere has to
wake the scheduler when it's over. Today that "somewhere" is N threads.
The claim of this doc is that it can be *one* thread, and that
everything the threads do *besides* waiting is Evident-expressible.

### Per-source anatomy

| Source | I/O syscall (FFI-able) | Parse + publish (FFI-able) | Irreducible Rust |
|---|---|---|---|
| `frame_timer` | `nanosleep` (via `thread::sleep`) | increment a counter → `tick_count` | thread + periodic wake |
| `stdin` | `read(2)` on fd 0 | split a line, strip `\r\n` → `stdin_line`/`stdin_seq` | thread blocked in `read_line` + wake |
| `file_line_reader` | `open` + `read(2)` on a path fd | split a line; EOF → `file_eof` | thread blocked in `read_line` + wake |
| `file_watcher` | `stat` (poll mtime) | mtime changed? → bump `file_changed` | thread + poll loop + wake |
| `wall_clock` | `clock_gettime`/`gettimeofday` | ms-since-epoch → `now_ms` | thread + periodic wake |
| `sigint` | — (`sigaction` install) | increment a counter → `signal_received` | **signal install + blocked-on-delivery thread** |

Read the columns: the first two are FFI + arithmetic + string ops —
exactly the toolkit `docs/design/ffi-os-evolution.md` shipped
(`Malloc`, `Read*`, `MonotonicTime`, `Sleep`, `packages/posix/file.ev`
already wraps `open`/`read`/`close` in Evident with zero Rust). The
third column is the same primitive in every row — "a thread that blocks
and wakes" — *except* sigint, whose first column is not I/O at all.

### Why sigint is the hard exception

sigint is the one row where the FFI-able columns are nearly empty.
There is no input to read and no payload to parse — just "a SIGINT
happened, bump a count." What it *does* need is process-global state
the runtime must own: installing a `SIGINT` disposition (`sigaction`)
and arranging for delivery to be observable. Today `signal_hook` does
this with a self-pipe + a thread blocked in `signals.forever()`.

You genuinely cannot run a Z3 solve from an async-signal-safe handler —
almost nothing is safe to call there — so the handler can only flip a
flag / write a byte; the *response* must run later on the scheduler
thread. But that's true of every source (none of them solve on their
own thread either). The decisive reasons sigint stays Rust are narrower
and more honest:

1. **There is no parse step to move.** The whole "source body" is
   `count += 1`. Porting it to an Evident FSM buys nothing — there's no
   I/O payload, no framing, no decode. The Evident half of the split is
   empty.
2. **Handler installation is process-global Rust state.** `sigaction` /
   `sigprocmask` mutate per-process disposition, not a per-resource fd.
   That belongs in the runtime, not in a typed-resource install.

(§ 2 notes that the *waiting* for a signal *could* be folded into the
awaiter via kqueue `EVFILT_SIGNAL` / Linux `signalfd` — turning a
signal into a readiness descriptor like any other. But since columns 1
and 2 are empty, folding only the wait would save one short thread at
the cost of process-global signal-mask handling in the awaiter. Not
worth it for v1. sigint stays a self-contained Rust source.)

---

## § 2 — The generic awaiter (the one irreducible Rust core)

### What it is

One Rust thread owning one kernel readiness object (kqueue on darwin,
epoll + timerfd on Linux). It holds a **registered set** of
await-descriptors:

```rust
enum AwaitKind {
    Readable { fd: RawFd },        // EVFILT_READ  / EPOLLIN
    Timer    { interval: Duration }, // EVFILT_TIMER / timerfd
    Vnode    { fd: RawFd },          // EVFILT_VNODE / inotify (file changed)
}

struct AwaitDescriptor {
    id:   u64,        // kevent.udata — maps back to the owning source
    name: String,     // SchedulerEvent::Tick { name } sent on fire
    kind: AwaitKind,
}
```

The loop is the entire irreducible core:

```rust
loop {
    let fired: Vec<u64> = kevent_block(&kq);   // sleeps until ≥1 descriptor ready
    for id in fired {
        let name = registry[&id].name.clone();
        if tx.send(SchedulerEvent::Tick { name }).is_err() {
            return; // scheduler gone
        }
    }
}
```

Crucially the awaiter **does not read, parse, or write the world.** It
only translates "kernel says descriptor X is ready" into "scheduler,
tick the FSM named X." The read/parse/publish happens on the scheduler
thread, in the Evident source FSM's tick, where it is safe because the
awaiter already guaranteed the operation won't block (the fd is
readable; the timer has fired).

This is a **readiness reactor** (epoll-style), not a completion
reactor. That distinction is what lets the FFI read live in Evident: by
the time the source FSM runs `read(fd, buf, n)`, the data is sitting in
the kernel buffer and the syscall returns immediately.

### How it replaces N `start(tx)` threads

| Today | With the awaiter |
|---|---|
| `frame_timer` thread: `loop { sleep; send }` | `Timer { interval }` descriptor |
| `stdin` thread: `loop { read_line; send }` | `Readable { fd: 0 }` descriptor |
| `file_line_reader` thread: `loop { read_line; send }` | `Readable { fd }` descriptor |
| `file_watcher` thread: `loop { sleep; stat; send }` | `Vnode { fd }` descriptor (event-driven, no poll) |
| `wall_clock` thread: `loop { sleep; send }` | `Timer { interval }` descriptor |

Five threads → five entries in one awaiter's registry, serviced by one
`kevent()` call. The per-source `WriteQueue` and the `drain_writes` /
`write_fields` trait methods disappear for every ported source: the
world write is no longer queued on a side channel by a thread, it is
produced by the source FSM's tick through the normal FSM → world-write
path the scheduler already applies.

### Integration with what exists

- **Wake channel — unchanged.** The awaiter sends the same
  `SchedulerEvent::Tick { name }` on the same `Sender<SchedulerEvent>`
  the threads use today. The scheduler's idle path
  (`scheduler.rs`: "no FSM scheduled this tick → block on
  `event_rx.recv()`") does not change at all. From the scheduler's
  point of view, wakes still arrive by name; it just doesn't care that
  one thread now produces them instead of five.
- **World writes — through the FSM, not the queue.** Because the source
  is now an FSM that writes `world.stdin_line = …`, the existing
  EVENT-write machinery (one observable write per tick) and read-set
  subscription wake-up apply with zero change. The `WriteQueue` side
  channel is retired for ported sources.
- **Halt — unchanged.** When every awaiter descriptor is gone (all fds
  closed / EOF, no timers) and sigint's thread has exited, all senders
  drop, `rx.recv()` returns `Err`, the scheduler halts clean — exactly
  today's "all sources dead" path.
- **The registry is built at install time (v1).** Every FTI
  await-declaration (§ 3) registers its descriptor before the scheduler
  starts, matching today's start-at-startup model. Mutating the set
  while the awaiter is blocked in `kevent()` (add/close an fd, change a
  timer) is the v2 command-channel concern (§ 3) and uses a
  self-wake (`EVFILT_USER` / a self-pipe) to break the block and
  re-read the registry.

### Cross-platform note

The repo targets darwin, and **kqueue covers all three descriptor kinds
natively**: `EVFILT_READ` (fd readable), `EVFILT_TIMER` (periodic,
replaces the sleep loop with no extra fd), `EVFILT_VNODE`
(`NOTE_WRITE|NOTE_DELETE|NOTE_RENAME` — replaces file_watcher's mtime
poll with a real event, a strict improvement). Recommend a hand-rolled
kqueue awaiter for darwin first — it matches the repo's
minimal-dependency posture (raw `z3-sys`, `libffi`) and is ~150 lines.

Linux needs three objects instead of one: `epoll` for fds, `timerfd`
for `Timer`, `inotify` for `Vnode`, all multiplexed under one
`epoll_wait`. That's a `#[cfg(target_os = "linux")]` second
implementation behind the same `AwaitDescriptor` interface (~250
lines). The `mio` crate abstracts fd readiness + timers across both,
but its vnode/timer story is thin and it's a non-trivial dependency;
hand-rolling darwin-first keeps the slice small and the dependency
budget intact. Caveat for `Vnode` on darwin: `EVFILT_VNODE` needs an
open fd, so a file that does not yet exist can't be watched directly —
fall back to a `Timer` + `stat` for the "watch a path that may appear
later" case (today's file_watcher already polls, so this is no
regression).

---

## § 3 — The FTI-v2 await-declaration surface

A typed resource today declares its setup as a one-shot
`install ∈ Seq(InstallStep)` (`stdlib/runtime.ev`):

```evident
enum InstallStep =
    Run(Effect)               -- fire, discard result
    Bind(String, Effect)      -- fire, capture result into the named field
```

`event_sources/declarative_install.rs` queries the type body under its
pins, decodes the `install` Seq, dispatches it atomically through the
scheduler's `DispatchContext` (so `ArgPriorResult(N)` threads handles
forward), and writes each `Bind`'d result to `<fsm>.<param>.<field>`.
This is exactly the hook the awaiter plugs into. We add one variant:

```evident
enum InstallStep =
    Run(Effect)               -- fire, discard result
    Bind(String, Effect)      -- fire, capture result into the named field
    Await(AwaitSpec)          -- register a readiness descriptor with the awaiter

enum AwaitSpec =
    AwaitReadable(Int)        -- wake this source when fd is readable
    AwaitTimer(Int)           -- wake this source every N ms
    AwaitVnode(Int)           -- wake this source when the fd's file changes
```

When `declarative_install` decodes an `Await` step, instead of
dispatching an effect it registers an `AwaitDescriptor` with the awaiter
(`name` = the source FSM, `kind` from the `AwaitSpec`). Bootstrapping
(`open` the file, get its fd) still uses `Bind`; the fd it produces is
the argument to `Await`. The source is otherwise an ordinary `fsm`: it
ticks when its descriptor fires, reads the previous buffer with `_buf`,
does the non-blocking FFI read, and publishes a world field.

### Concrete: stdin generalized to a file-handle reader (stdin = fd 0)

The headline dedup — today's `stdin.rs` and `file_line_reader.rs` are
the same loop on a different fd. One Evident type covers both:

```evident
-- A generic line reader over any fd. stdin is just fd 0.
-- `open` is omitted for fd 0 (it's already open); a file source
-- Binds fd = open(path) first, then awaits readiness on it.
fsm FdLineReader(fd ∈ Int, buf ∈ String, halt ∈ Bool)
    line     ∈ String      -- world field this source publishes
    seq      ∈ Int
    file_eof ∈ Bool

    -- Register interest once; the awaiter wakes us on readable/EOF.
    install ∈ Seq(InstallStep) = ⟨AwaitReadable(fd)⟩

    -- On each wake the fd is readable, so this read does not block.
    -- Read available bytes into a scratch buffer, append to the
    -- carry-over buffer, then frame complete lines out of it.
    scratch ∈ Int = Malloc(4096)
    n       ∈ Int = LibCall("libc", "read", "i(ipi)",
                            ⟨ArgInt(fd), ArgHandle(scratch), ArgInt(4096)⟩)
    chunk   ∈ String = (n > 0 ? ReadStr(scratch, 0) : "")
    pending ∈ String = _buf ++ chunk

    -- Frame: if pending has a newline, publish up to it, carry the rest.
    nl ∈ Int = indexof(pending, "\n")
    line = (nl ≥ 0 ? substr(pending, 0, nl) : _line)   -- publish a full line
    buf  = (nl ≥ 0 ? substr(pending, nl + 1, #pending) : pending)
    seq  = (nl ≥ 0 ? _seq + 1 : _seq)

    -- read() == 0 is EOF; publish it and ask to stop.
    file_eof = (n = 0)
    halt     = (n = 0)
```

A file source is the same type with one extra install step to obtain
the fd:

```evident
fsm FileReader(path ∈ String, fd ∈ Int, buf ∈ String, halt ∈ Bool)
    ..FdLineReader     -- same read/frame/publish body
    install ∈ Seq(InstallStep) = ⟨
        Bind("fd", LibCall("libc", "open", "i(si)",
                           ⟨ArgStr(path), ArgInt(0)⟩)),  -- O_RDONLY
        AwaitReadable(fd)                                 -- fd from the Bind above
    ⟩
```

(`indexof` / `substr` / `#str` are the string ops that shipped in
session GAPC — see `runtime/src/translate/exprs/string_ops.rs`. The
line-framing that `BufReader::read_line` did quietly in Rust is now
explicit Evident; § 5 flags this as the main place correctness has to be
re-earned.)

### Concrete: the frame timer

Pure — the timer firing *is* the event, no syscall in the body:

```evident
fsm FrameTimerSource(tick_count ∈ Int)
    -- EVIDENT_TICK_MS sets the interval; default 100ms (runtime supplies it).
    install ∈ Seq(InstallStep) = ⟨AwaitTimer(16)⟩
    tick_count = _tick_count + 1
```

wall_clock is the same with one FFI read in the body
(`now_ms = MonotonicTime` or a `gettimeofday` `LibCall`); file_watcher
is `AwaitVnode(fd)` with `file_changed = _file_changed + 1`.

### Tie to the FTI-v2 bidirectional command channel

CLAUDE.md notes v1 sources are push-only (events flow source → owner)
and "v2 will add bidirectional command channels (mode switching,
explicit reads, seeks, close)." The awaiter is exactly the substrate for
that. The `install`-time `Await` declarations are the **read side** —
the source subscribes to readiness. The **write side** is the same
source FSM *emitting* effects that re-configure the registered set:

```
Unawait(fd)          -- close: drop the descriptor, close the fd
SetTimer(id, ms)     -- mode switch: change a timer's interval
Seek(fd, offset)     -- explicit reposition before the next read
```

These dispatch like any effect, mutate the awaiter's registry, and wake
the awaiter out of `kevent()` via `EVFILT_USER` / a self-pipe so it
re-reads the set. v1 ships the read side (static registry built at
install); the command side is a clean extension that does not touch the
core loop, only how the registry is mutated.

---

## § 4 — Which sources port, which stay

### Port to Evident (awaiter descriptor + FFI body)

- **frame_timer** → `AwaitTimer`. Body is `tick_count = _tick_count + 1`
  — no syscall at all; the timer firing is the wake. Pure.
- **stdin** → `FdLineReader` over fd 0. `AwaitReadable(0)` + the
  read/frame body above.
- **file_line_reader** → the *same* `FdLineReader`/`FileReader`,
  fd from `open(path)`. This is the dedup payoff: two ~150-line Rust
  files become one Evident type.
- **file_watcher** → `AwaitVnode(fd)` (darwin: real event, no poll;
  Linux: inotify; pre-existence fallback: `AwaitTimer` + `stat`). Body
  is `file_changed = _file_changed + 1`.
- **wall_clock** → `AwaitTimer` + a `MonotonicTime`/`gettimeofday` read.
  Body publishes `now_ms`.

Each ported source loses its `WriteQueue`, `start`/`stop`/`Drop`, and
`EventSource` impl; it gains an Evident type (~15–40 lines) and an entry
in the awaiter registry.

### Stays Rust

- **sigint** — justified in § 1: the FFI-able columns are empty (no
  payload to read, no parse), and handler installation is process-global
  Rust state (`sigaction`/`sigprocmask`). Porting buys nothing and would
  push signal-mask handling into the awaiter. It remains a self-contained
  Rust source on the same `SchedulerEvent` channel. (The awaiter could
  absorb its *wait* via `EVFILT_SIGNAL`/`signalfd`; deferred — not worth
  the process-global mask handling for one counter.)
- **reflection.rs** — not an I/O source. It encodes the loaded `Program`
  AST (via the runtime-internal `encode_program`) and writes it once to a
  `∈ Program` world field. It has no syscall, no waiting, and depends on
  runtime-internal state Evident can't reach. It is runtime reflection
  machinery that happens to ride the source interface for its one-shot
  write; it stays.
- **declarative_install.rs** — the FTI install *dispatcher*. It is the
  machinery the ported sources register *through* (§ 3 extends it with
  the `Await` step). It is infrastructure, not a source; it stays and
  grows slightly.

The split is clean: everything that *waits on I/O and parses a payload*
ports; the one source with no payload (sigint) and the two pieces of
runtime machinery (reflection, declarative_install) stay.

---

## § 5 — What it costs + first slice

### New primitives needed

1. **The awaiter** — one Rust module (`event_sources/awaiter.rs`): the
   `AwaitDescriptor` registry, the kqueue setup, the block-and-wake
   loop, the `Sender<SchedulerEvent>` hookup. ~150 lines darwin-only;
   ~250 with the Linux `cfg` arm.
2. **The `Await` install step** — the `Await(AwaitSpec)` `InstallStep`
   variant (`stdlib/runtime.ev`) + the decode/register arm in
   `declarative_install.rs` that routes an `Await` step to the awaiter
   registry instead of `dispatch_all`. ~50–80 lines Rust + a few lines
   Evident.

No new *effect* primitive is required for the read side — `Malloc`,
`Read*`, `LibCall("libc", "read", …)`, and the string ops all already
exist. The v2 command side adds `Unawait`/`SetTimer`/`Seek` effects when
that arc is taken; out of scope for the first slice.

### The LOC trade — modest Rust shrink, real architectural win

Ported-away Rust (current line counts):

| File | Lines |
|---|---|
| `frame_timer.rs` | 123 |
| `stdin.rs` | 139 |
| `file_line_reader.rs` | 160 |
| `file_watcher.rs` | 119 |
| `wall_clock.rs` | 112 |
| **total removed** | **~653** |

Replaced by: the awaiter (~150 darwin / ~250 +Linux) + the `Await`
plumbing (~70) = **~220–320 Rust**, plus ~120–180 lines of *Evident*
source defs (which don't count against the runtime LOC budget). The
`WriteQueue`/`drain`/`drain_writes`/`write_fields` machinery in `mod.rs`
also shrinks once only sigint + the one-shot bridges use it.

So: a **real Rust shrink of roughly 330–430 lines**, larger if Linux is
deferred. But the LOC number undersells it — the structural wins are the
point:

- **5 bespoke threads → 1 reactor.** One place owns "block off-thread
  and wake," instead of six copies of it.
- **Two readers → one.** stdin and file collapse into `FdLineReader`
  (stdin = fd 0), the cleanest expression of "they were always the same
  loop."
- **Source logic becomes data.** A new source is an Evident type a user
  can read, fork, and extend — not a Rust file with a thread and a
  `Drop` impl. This is the "prefer Evident over Rust" line held at the
  source layer, and it directly serves the ≤11K-LOC runtime target
  (`docs/design/minimal-runtime.md`).
- **The write side-channel dissolves.** Ported sources publish through
  the ordinary FSM → world-write path; the `WriteQueue` exists only for
  the residual Rust sources.

### Smallest first slice (recommended)

Build the generic awaiter (darwin/kqueue, `Readable` + `Timer`) **plus
exactly one ported source — `FdLineReader`** — as proof. It is the
highest-value slice: it exercises fd readiness (the hard part), it
delivers the stdin+file dedup, and it has a ready-made oracle (the
existing stdin / file demos must produce identical output). Keep all
other sources on the current thread path during the slice — the awaiter
and the legacy threads coexist behind the same `SchedulerEvent` channel,
so the migration is incremental and reversible. Port frame_timer,
wall_clock, file_watcher one at a time afterward, each validated against
its existing demo.

### Risks

- **Cross-platform.** The awaiter is the one place platform code
  concentrates. Mitigate: darwin-first hand-rolled kqueue; Linux behind
  `cfg` as a follow-up. Don't reach for `mio` unless the Linux arm
  proves painful.
- **The FFI read+parse loop in Evident.** `BufReader::read_line` did a
  lot quietly: partial reads, line framing across wakes, `\r\n`
  stripping, UTF-8 boundary handling, EOF. That logic now lives in the
  source FSM's state (`buf` carry-over) and must be re-earned with the
  string ops. This is where bugs will concentrate; the `FdLineReader`
  slice exists to shake them out against a known-good oracle before any
  other source ports.
- **Readiness storms / busy-spin.** A level-triggered `EVFILT_READ` on
  an always-readable fd re-fires every loop. Use `EV_CLEAR`
  (edge-trigger) and have the source body drain all available bytes per
  wake, so a single wake fully consumes what's ready.
- **fd lifetime.** The fd opened in `install` must outlive the source
  and be closed on halt. Tie it to the `HandleRegistry` drop fn (the
  same mechanism `Malloc`/`FFIOpen` use), and have `Unawait`/halt close
  it.
- **sigint left out.** The awaiter is therefore not the *sole* async
  path — the scheduler still multiplexes awaiter wakes and the sigint
  thread on one channel. That's fine (it's the same channel today); it
  just means "one reactor" is "one reactor + one signal thread," which
  the halt logic already handles via sender-drop.

### Recommendation

Build it, darwin-first, in the slice order above. The LOC win is modest
but real; the architectural win — one reactor, sources-as-data, the
stdin/file dedup, the write side-channel retired — is decisive and
squarely on the runtime's stated trajectory. sigint stays Rust; that's a
feature of the split, not a gap in it.
