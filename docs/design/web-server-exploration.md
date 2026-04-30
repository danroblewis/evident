# Web Server in Evident — Problem Exploration

This document explores what it would take to write a web server primarily in Evident.
It is not a plan. It is an honest accounting of the problem.

---

## The Core Tension

Evident is a **declarative constraint language**: programs define what is true,
the solver finds witnesses, and the order of constraints doesn't matter.

A web server is **fundamentally imperative**: bind a port, accept a connection,
read bytes, parse them, route to a handler, compute a response, write bytes back.
Order matters completely. Side effects are the whole point.

These two things are in genuine conflict. Every logic/constraint language in
history that has faced this tension — Prolog, Mercury, miniKanren, MiniZinc,
Datomic, Haskell — has resolved it in one of two ways:

1. **Abandon purity** for I/O (Prolog's assert/retract model). Works, but loses
   the guarantees that make the language interesting.

2. **Hard boundary** between pure computation and effects (Haskell's IO monad,
   Mercury's world-threading, Erlang's process isolation). Pure code cannot
   perform effects; effects cannot be called from pure code. The type system
   (or process structure) enforces the wall.

A third weaker option — **delegate I/O entirely to the host** (miniKanren,
MiniZinc, Souffle) — is the most pragmatic starting point. The constraint engine
is a library; a thin shell handles all I/O. This is essentially what the Evident
IDE already does: FastAPI is the shell, Evident is the oracle.

---

## What We Think We Need

### 1. String Operations

**Current state:** Strings exist as Z3 `StringSort` values. You can assert string
equality, use them in constraints, store them in bindings. You cannot do anything
with them operationally.

**What we think we need:**
- Concatenation: `"HTTP/1.1 " ++ status_line ++ "\r\n"`
- Length: `|s|` or `length(s)`
- Substring / contains: `path starts_with "/api"`
- Splitting: parse a raw HTTP request into method, path, headers, body
- Pattern matching on string content: route `"/users/{id}"` extracting `id`

Without string operations, you cannot parse an HTTP request or construct an HTTP
response. This is the single most blocking gap for anything web-related.

---

### 2. Sequential Execution

**Current state:** None. Body items in a schema are unordered constraints. There
is no notion of "do this, then do that with the result."

**What we think we need:** Some form of sequencing. The shape of HTTP handling
is a pipeline:

```
raw bytes → parse → validate → route → handle → serialize → raw bytes
```

Each step depends on the output of the previous step. In a purely constraint-based
model you could express the *relationships* between these values (what a valid
parse looks like, what a valid route match looks like, etc.) but you cannot express
the *execution order*.

Options we're aware of:
- **Monadic do-notation**: `parse bytes >>= validate >>= route >>= handle`
- **Linear world threading** (Mercury): thread a world token through operations
- **`action` blocks**: a new keyword for sequenced effectful computation
- **Just leave sequencing to the host**: Evident solves constraints, Python runs the loop

We don't know yet which is right for Evident's design.

---

### 3. I/O Primitives

**Current state:** None inside the language. The CLI runtime has `socket` (sort of)
in the form of the HTTP server wrapping the solver, but that's Python, not Evident.

**What we think we need** (for Evident to own the server loop):
- Accept a TCP connection
- Read bytes from a socket
- Write bytes to a socket
- Possibly: timers, TLS, backpressure

The deeper question is whether Evident should own this at all. The oracle model
(Python handles sockets, Evident handles logic) may be the right permanent
architecture, not just a stepping stone.

---

### 4. Conditional Dispatch

**Current state:** We have `⇒` (implication), `∧`, `∨`. We do NOT have
`if/then/else` or `match/case` on values. You can express "if A then B must hold"
but you cannot express "if method=GET then execute this handler, else that one"
as a selection between alternatives.

**What we think we need:**

```
-- select one handler based on request
handler = match (request.method, request.path) {
    (GET, "/")       => home_handler
    (GET, "/hello")  => hello_handler
    (POST, "/data")  => data_handler
    _                => not_found_handler
}
```

We already have named sets and tuple relations which can express routing tables.
But dispatching to different *computation paths* based on the match result is
still missing.

---

### 5. Mutable State Between Requests

**Current state:** `assert` adds ground facts permanently for the session. There
is no way to update a value, no transactional state, no per-request isolation.

**What we think we need:**
- Some notion of server state that persists across requests (session counts,
  user records, connection pools)
- The ability to read state, compute a new state, and atomically write it back
- Per-request isolation so concurrent requests don't corrupt each other

The research suggests **Datomic's model** is the most compatible with Evident:
the evidence base is append-only (monotonic), and "updates" are new assertions.
The current state of anything is derived by applying all forward rules to the
full history of assertions. This fits Evident's design almost perfectly — we
already have forward rules (`A, B ⇒ C`) for deriving consequences.

---

### 6. Error Handling / UNSAT Recovery

**Current state:** If a query is UNSAT, it returns `satisfied=False`. There is
no way to catch that and try an alternative within the language itself.

**What we think we need:**
- `? ClaimA or else ? ClaimB` — try A, fall back to B if UNSAT
- A way to say "if this request doesn't match any route, return 404" that is
  expressed *in Evident* rather than in the host shell

This also connects to HTTP error responses: a bad request should produce a 400
response schema, not just fail. Currently you'd need the Python shell to handle
that.

---

### 7. JSON / Wire Format Serialization

**Current state:** None. We can construct response schemas (status code, headers,
body), but we cannot serialize them to bytes. Even converting the enum `OK` to
the string `"200 OK"` requires string operations we don't have.

**What we think we need:**
- Enum-to-string mappings (could be expressed as tuple relations once string
  operations exist)
- Record/schema-to-JSON serialization
- Base64, URL encoding, header parsing

---

## The Architecture Convergence

After researching Prolog, Mercury, miniKanren, Haskell, Erlang, Datomic, and
MiniZinc, they all converge on the same shape when stripped down:

```
[ I/O Shell ]          ← reads/writes bytes, manages sockets
      ↕
[ Constraint Oracle ]  ← pure: validates, routes, authorizes, generates response schemas
      ↕
[ State Store ]        ← append-only facts + forward-rule derivation
```

Evident naturally owns the middle. The question is how much of the top and
bottom it should own.

**The shell/oracle split (Phase 1 thinking):**
The Evident IDE already IS this architecture. FastAPI is the I/O shell; the Z3
solver is the oracle. A web server built this way would have Python own the
sockets and Evident own the routing, validation, and response shapes. The
"program" is primarily Evident; the I/O glue is minimal Python.

**The pure Evident server (Phase 2 thinking):**
For Evident to own the full server loop, it needs an `action` concept — a
construct that is sequenced and effectful, but whose preconditions are checked
by the constraint solver. Something like:

```
action handle_request
    req ∈ HttpRequest
    valid_request req       -- Evident checks this before proceeding
    response ∈ HttpResponse
    response.status = route_to_status(req)
    send connection response -- effectful
```

This is the Haskell WAI architecture (`Request -> IO Response`) translated to
Evident's vocabulary. The constraint solver gates the effect.

---

## Open Questions

1. **Should Evident own I/O at all?** The oracle model (Python shell + Evident
   logic) may be the right permanent answer, not a temporary workaround. The
   question is whether "Evident runs the web server" matters or whether "Evident
   IS the web server's brain" is sufficient.

2. **What does a valid Evident `action` look like?** If we add sequencing, how
   does it interact with the constraint model? Can you put constraint checks
   inside an action? Can you backtrack inside an action?

3. **Is the evidence tree the right execution trace?** The `Evidence` tree already
   records which sub-claims were used to satisfy a query. Could this become an
   execution trace for a server request? "Request was handled because: route matched,
   auth passed, handler returned 200."

4. **How does concurrency fit?** Multiple simultaneous requests need some isolation
   model. Erlang's processes, Haskell's `TVar`, or simply separate EvidentRuntime
   instances per request (which is what the IDE does today).

5. **What is the "main" of an Evident server program?** In Python it's `if __name__
   == '__main__'`. In C it's `int main()`. For Evident, is it a special schema?
   A `?` query? An `action`?

---

## Related Prior Art in Evident

The `examples/04-api-validation.md` design doc (pre-runtime) already sketched
this problem: HTTP request validation using claims, token parsing as a sub-claim,
scope checking as an implication. That vision is now realizable for the
*validation* side. The gap is everything around it: parsing, routing, I/O.

The `beavers.ev` example already demonstrates the decomposition pattern:
layered claims that compose into a top-level claim, where the solver checks
all constraints jointly. A web server handler could be structured identically:
`claim handle_order_creation` composes `valid_auth`, `valid_body`, `idempotent`,
`within_rate_limit` — the solver either confirms the request is handleable or
returns UNSAT (which the shell translates to an error response).

---

*Last updated: exploration phase — no implementation decisions made.*
