# Sockets and the HTTP Server Plugin

## Purpose

Demonstrate that Evident's plugin model and synchronous-reactive execution
work end-to-end on a real network protocol. The minimum target: a
long-running process that responds to `curl http://localhost:PORT/` with
`HTTP/1.0 200 OK` and a fixed body.

This is the first plugin that handles *multiple* I/O streams. Stdin and
SDL each had one input source; sockets have N (one listening socket plus
many client connections). How that multi-stream concurrency lives entirely
inside the plugin — without leaking into the language — is what this
exercise validates.

For the broader concurrency rationale, see
`docs/design/synchronous-reactive-concurrency.md`.

---

## Two Possible Shapes

A server can be modelled in Evident two ways. We commit to the first now;
the second is a future exercise.

**Approach B (this design): plugin multiplexes, Evident handles one
request at a time.** Equivalent to a WSGI app behind a gateway. The
plugin owns the listening socket, accepts connections, buffers partial
requests, and at most once per tick presents *one* fully-received request
to Evident. Evident produces a response. Plugin sends it and closes the
connection.

**Approach A (later): Evident sees `Seq(Connection)` directly.** The
constraint model describes the behaviour of every active connection at
once. Necessary for proxies, gateways, load balancers — anything where the
right answer depends on the whole connection set. Requires richer string
parsing than we have today and more sophisticated state representation.

Approach B is the WSGI/ASGI shape — the well-known, well-understood one
that most actual web servers expose to their applications. Choosing it for
the first cut keeps the moving pieces small and proves the sockets plugin
in isolation.

---

## Evident-Side Types

Everything Evident sees is in `stdlib/sockets.ev`:

```evident
type HTTPRequest
    has_request ∈ Bool      -- false when no client is ready this tick
    method      ∈ String    -- "GET", "POST", ...
    path        ∈ String    -- "/", "/hello", ...

type HTTPResponse
    status ∈ Nat            -- 200, 404, ...; 0 means "no response"
    body   ∈ String

type HTTPServer
    request  ∈ HTTPRequest
    response ∈ HTTPResponse
```

A program declares `server ∈ HTTPServer` in `main`. Each tick, the
plugin fills in `server.request.*` (with `has_request = false` when no
client is ready) and Evident's constraints determine `server.response.*`.

The `has_request = false` case must be handled — the constraint model has
to say "when no request, no response" — otherwise Evident's response is
free and the plugin sends garbage to whatever client comes next.

```evident
type main
    server ∈ HTTPServer

    server.request.has_request ⇒ (
        server.response.status = 200 ∧
        server.response.body   = "Hello from Evident\n"
    )
    ¬server.request.has_request ⇒ (
        server.response.status = 0 ∧
        server.response.body   = ""
    )
```

The `status = 0` sentinel is how the plugin knows there's nothing to
send. It's the same pattern as `w = 0 ∧ h = 0` for "skip this rect" in the
SDL plugin.

---

## Plugin Contract

`runtime/src/plugins/sockets.py:HTTPServerPlugin`:

- `handles_types = {'HTTPServer'}`
- Construction: `__init__(host='127.0.0.1', port=8080)` — config from CLI
- `start()`:
  1. Open AF_INET TCP listening socket
  2. `setsockopt(SO_REUSEADDR)` to avoid "address in use" on restart
  3. `bind((host, port))`, `listen(backlog)`
  4. Set non-blocking
  5. Initialize empty connection table: `{fd: ClientState}`
- `before_step(state) → dict`:
  1. Build the read-set: listening socket + every active client fd
  2. `select.select(rset, [], [], timeout=0.05)` — short timeout so the
     loop can still tick when nothing's happening
  3. If listening socket is readable: `accept()`, add new client to table
     in `Reading` state with empty buffer
  4. For each readable client: `recv(4096)`. If empty bytes → connection
     closed, drop it. Otherwise append to buffer.
  5. Find the *first* client whose buffer contains `\r\n\r\n` (end of
     request headers). Parse its first line: `<METHOD> <PATH> HTTP/...`.
     Mark that client as the "current request" for this tick.
  6. Return `{server.request.has_request: True/False, server.request.method:
     ..., server.request.path: ...}`. When False, the other fields are
     empty strings.
- `after_step(bindings) → bool`:
  1. If no current request this tick → return True (continue)
  2. Read `server.response.status` and `server.response.body`. If
     `status == 0` → close the current request's connection without
     sending (Evident said "no response").
  3. Otherwise: format `"HTTP/1.0 {status} OK\r\nContent-Length: {len}\r\n\r\n{body}"`
     and `send()` it. Close the connection.
  4. Return True (continue)
- `stop()`: close all client sockets, close listening socket

The plugin holds the only reference to client sockets. Evident never sees a
file descriptor or a partial buffer. From Evident's perspective there's a
single "current request" each tick that may or may not exist.

### Why one-request-per-tick

We could accept up to N requests per tick and present them as a small
batch. We don't, for two reasons:

1. **Approach B's whole point** is to look like a WSGI app — one request
   in, one response out. Batching breaks that mental model.
2. The Evident program doesn't have a way today to express "compute a
   response per element of `Seq(Request)`" cleanly without nested types
   we'd rather not build for the demo.

The throughput cost: at 50-200ms per tick we serve 5-20 req/s. Fine for
the demo. The interesting use case (Approach A) doesn't have this limit
because every connection is in scope simultaneously.

### Connection lifecycle

HTTP/1.0 closes per request. The plugin closes the client socket
immediately after sending the response. No keep-alive, no request
pipelining, no chunked transfer. This keeps the buffering logic to "read
until \r\n\r\n appears, then we have a complete request". Good enough for
curl.

### Error handling

- `socket.error` on accept/recv/send → drop that connection, keep going
- Bind failure → fail loudly at startup with a clear error
- Malformed request line → respond `400 Bad Request` and close
- Plugin shutdown → close every socket in `stop()`'s finally branch

---

## CLI

`evident execute` gains two flags consumed by the sockets plugin:

```
--host HOST   bind address (default 127.0.0.1)
--port PORT   bind port (default 8080)
```

Like `--width`/`--height`/`--title` for SDL, these flags are always
accepted; they're only consumed when `HTTPServer` is in the active plugin
list.

---

## Test Plan

Manual:
1. Terminal 1: `python evident.py execute programs/http_demo/server.ev --port 8080`
2. Terminal 2: `curl -v http://localhost:8080/`
3. Expect: `200 OK`, body `Hello from Evident\n`
4. Several requests in a row: all succeed, no errors in server output
5. Kill server with Ctrl-C: clean shutdown, port freed

Automated conformance test in `tests/conformance/test_http_server.py`:

1. Spawn the demo as a subprocess on a random free port
2. Wait for the port to accept connections (poll with timeout)
3. Issue a GET via stdlib `urllib`, assert 200 + body
4. Send a few concurrent requests, assert all return 200
5. Terminate the subprocess, assert port released

Test must be skipped if `pysdl2` is missing (it's not, but the
conformance suite shouldn't grow new system-level dependencies).
Actually sockets need no extra deps — only `select` and `socket` from
stdlib. So no skip condition.

---

## What This Validates

- **Plugin isolation:** the sockets plugin uses `select` internally and
  Evident never knows.
- **Multi-stream concurrency in plugin layer:** N connections, one at a
  time presented to the language.
- **Long-running process:** like SDL, but driven by network I/O instead
  of frame timing.
- **Request → response shape:** the WSGI/ASGI handler pattern, with the
  constraint solver as the handler.

If this works, the next interesting program is Approach A — a constraint
model that sees the whole connection set and expresses something only
declarative concurrency makes easy. That's the proxy, the gateway, the
collapse-forwarding cache. But first the sockets plugin has to exist.

---

## Out of Scope

- HTTP/1.1 keep-alive, pipelining, chunked transfer
- TLS / HTTPS
- Request body parsing (POST data, form data, JSON)
- Routing (every request gets the same response)
- Multiple servers in one program
- Outbound HTTP (the plugin is server-only; client connections come later)

All addressable later, none needed for the curl demo.
