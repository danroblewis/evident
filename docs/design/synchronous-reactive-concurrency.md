# Concurrency in Evident: The Synchronous-Reactive Model

## The Question

Most languages give programmers a menu of concurrency primitives — threads,
processes, coroutines, light threads, virtual threads, callbacks, event loops,
epoll. Each comes with its own coordination story: locks, channels, futures,
async/await.

What does Evident, a constraint-solving language, provide?

The answer is: **none of those, and we shouldn't add them.** Evident's
concurrency model falls out of how the solver works. It belongs to a
different family of languages — synchronous reactive — and the implications
for I/O, plugins, and what kinds of systems Evident can express run deep
enough to deserve a written record.

---

## Why Imperative Concurrency Primitives Don't Apply

Every primitive on the standard menu solves a problem rooted in **stepwise
imperative execution**:

| Primitive | Solves |
|---|---|
| Threads / processes | Doing two things "at once" on separate cores or in interleaved time slices |
| Coroutines / async | Yielding control while waiting for I/O without blocking the thread |
| Locks / mutexes | Two threads racing to read-modify-write the same memory |
| Channels / queues | Passing messages between independent units of execution |
| Event loops / epoll | Multiplexing I/O readiness across many file descriptors |

Notice the underlying assumption: **execution can be paused and resumed**, and
between any two steps the program may have an inconsistent intermediate
state. All of those primitives exist to manage that intermediate state.

A constraint solve has no intermediate state. The Z3 query is "find a
satisfying assignment for these constraints." It either finishes or it
doesn't. There is no observable point partway through. No two "threads" can
be inside the same solve at once because there's no inside.

This means the entire problem the menu solves doesn't exist for us.

---

## The Synchronous-Reactive Model

Evident is in the same family as **Esterel, Lustre, Céu, Verilog, VHDL, and
StateCharts** — the synchronous reactive languages used for hardware design
and embedded control systems. Their shared model:

1. Time is divided into discrete **ticks** (sometimes called "instants").
2. At each tick, **all signals/values are evaluated together**.
3. The system advances **atomically**: input → solve → output, in one logical step.
4. Between ticks the program does nothing; within a tick the program produces
   a complete, consistent next state.

Evident's executor loop is exactly this:

```
while running:
    given = collect_events_from_all_plugins()    # the inputs of this tick
    bindings = solve(main, given + current_state) # one atomic instant
    apply_outputs_to_all_plugins(bindings)        # side effects
    current_state = next_state(bindings)
```

There are no threads because the language can't express interleaved
execution. There are no locks because there's no shared mutable state.
There are no race conditions because there's no race. Programs are
**correct-by-construction with respect to concurrency**.

This is not weaker than imperative concurrency — it's a different shape of
power. The cost of giving up "I can fire off ten goroutines" is buying
"every program is automatically free of data races."

---

## Implications for Plugins

The plugin layer is where Evident meets the operating system. Therefore the
plugin is also where any **physical** concurrency lives.

A sockets plugin can use `select()`, `epoll()`, `kqueue()`, or threads
internally — whatever the OS gives it for multiplexing I/O. None of that is
visible to the Evident program. The plugin's job each tick is:

1. **`before_step`** — gather every event that arrived since the last tick
   (new connections, bytes ready to read, timers fired) and present them as
   a single batch of given values.
2. After Evident solves, **`after_step`** — execute every side effect the
   solved state implies (write bytes, close connections, open new ones).

The plugin says "since you last ran, these N things happened." Evident
responds "given those N things and the current state, here is the entire
next state." Plugin executes. Repeat.

This is the same shape WSGI/ASGI use, scaled up. WSGI defines `app(env,
start_response)` as a single synchronous request handler; the gateway hides
all concurrency. ASGI does the same with async coroutines. Evident does the
same with constraint solves. **The plugin is the gateway. The Evident
program is the app.**

---

## What This Lets Us Express

Consider a collapse-forwarding HTTP proxy: 100 clients ask for the same
URL, and the proxy makes one upstream request and shares the response with
all 100. This requires seeing every active connection at once.

In Evident this is natural:

```evident
type main
    clients   ∈ Seq(ClientConn)
    upstreams ∈ Seq(UpstreamConn)
    -- For each client requesting URL X with no in-flight upstream,
    -- the next state has an upstream for X.
    -- For each client waiting on an upstream that just returned,
    -- the next state delivers the response.
    -- All happens in one tick.
```

The solver searches a satisfying assignment for the entire next state. There
are no race conditions because there is no race — the model treats all
clients and all upstreams as one set of constraints. No locks. No deadlock.
Just a relation between current state and next state.

The same shape applies to load balancers, gateways, multiplexers,
state-machine-heavy embedded controllers, transaction coordinators, and
anything else where "the right answer depends on seeing the whole system."
That's the class of problems synchronous reactive languages were invented
for. It's the class Evident is good at.

---

## What This Cannot Express Well

There are problems where the synchronous-reactive model is genuinely
awkward:

**Long-running blocking computations** that shouldn't pause the tick rate.
A tick has to finish before the next one starts. If the model includes a
constraint that requires Z3 to take 30 seconds, every plugin's I/O is
delayed by 30 seconds. Solution: the plugin does long work in the
background and presents results when they're ready. Evident never sees the
30-second wait — it just sees a "result available" event in some later tick.

**Independent worlds.** If you wanted truly independent subsystems with no
interaction, you wouldn't model them in one Evident program — you'd run
multiple Evident processes communicating through plugins. The boundary
between processes is the only place "real" concurrency lives.

**Sub-tick precision.** Evident can't say "two events happened in this
order within the same tick." Within a tick everything is simultaneous. If
order matters, it has to be explicit in the model (sequence numbers,
timestamps as part of the data).

These are real limitations. They're also the limitations of every
synchronous-reactive language. They exist because of the model's
fundamental simplicity — and that simplicity is the whole point.

---

## Throughput Reality

Constraint solves take 50–200ms in practice. That puts a hard ceiling on
tick rate: 5–20 ticks per second. Each tick can process hundreds of events
in parallel, so total event throughput is meaningful, but per-event latency
is bounded by tick time.

Evident will not beat nginx (microsecond per-request latency) or Go (millions
of QPS). For many problems this matters; for many it doesn't. The point of
expressing a system in Evident is correctness, clarity, and the ability to
reason about it as a constraint relation — not raw throughput. When
throughput matters more than expressiveness, write the system in something
else and have Evident drive policy or audit behaviour.

---

## What We're Not Adding

Concretely:

- No `async` / `await` keywords.
- No threads, processes, or process spawning at the language level.
- No channels, queues, or message passing primitives.
- No locks, mutexes, semaphores, or atomics.
- No `select` / `poll` / event-loop primitives at the language level.

All of those things may exist *inside plugins* (in fact a sockets plugin
will use `select` internally). None of them appear in Evident source.

---

## What We Are Adding

The sockets plugin and the constraint-modeled servers it enables.

The first server is the simple shape — Approach B, one request per tick,
plugin handles multiplexing, Evident handles request → response. This is
the WSGI-app pattern. It proves the plugin works and gives us a curl-able
demo.

Once that ships, the more interesting shape — Approach A, with `clients`
and `upstreams` as `Seq(Connection)` in `main` — becomes the natural way to
express proxies, gateways, and other multi-stream systems. That's the
version that demonstrates what synchronous-reactive constraint modeling can
do that other languages can't easily do.

Both shapes use the same plugin and the same executor. The difference is
how much of the system the Evident program describes.
