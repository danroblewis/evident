# The TCPSocket Primitive

## Purpose

Expose raw TCP transport to Evident programs. The plugin handles
listen/accept/read/write/close — nothing more. Any protocol on top
(HTTP, line-oriented chat, custom binary) is implemented in Evident
itself, by reading received bytes and writing response bytes through
the socket.

This is deliberately the *opposite* of giving Evident an `HTTPRequest`
type. The whole point of a constraint language is to express
relationships between values; baking HTTP into the plugin means the
language never gets to express any of HTTP. With a raw socket primitive,
HTTP framing — `received contains "\r\n\r\n"`, response construction
via string concat — happens where it should: in the constraint program.

For the broader concurrency rationale, see
`docs/design/synchronous-reactive-concurrency.md`.

---

## The Type

`stdlib/tcp.ev`:

```evident
type TCPSocket
    has_conn ∈ Bool      -- plugin: a client is currently connected
    received ∈ String    -- plugin: every byte received from this client so far
    out      ∈ String    -- evident: bytes to write THIS tick
    close    ∈ Bool      -- evident: close the connection after sending
```

Two fields are written by the plugin (`has_conn`, `received`), two are
read by the plugin (`out`, `close`). This is the same shape as Stdin's
`(char, eof)` and Stdout's `(out)` — Evident sees a snapshot, sets the
outputs, plugin executes the side effects.

`out` and `close` correspond per-tick: whatever Evident sets in `out`
this tick is sent this tick, then if `close` is true the connection
closes. Next tick `out` is whatever Evident decides for that tick (often
empty while waiting for more bytes).

---

## Single-Connection Semantics

This first cut accepts **one client at a time**. While a client is
connected, later client connect attempts queue in the OS backlog. The
plugin only accepts the next one after the current connection closes.

This is the equivalent of Stdin/Stdout — one input stream, one output
stream — applied to TCP. It keeps the primitive small and gives Evident
a single `received` buffer and a single `out` field.

True multi-connection concurrency in the language (`Seq(TCPSocket)`)
is the future story. It's where the synchronous-reactive model
distinguishes itself from imperative servers — a constraint over the
whole connection set replaces locks, queues, and per-connection
state machines. But it's not needed for the curl-able demo.

---

## Plugin Lifecycle

`runtime/src/plugins/tcp.py:TCPSocketPlugin`:

- `start()` opens the AF_INET TCP listening socket on `(host, port)`,
  sets `SO_REUSEADDR`, `bind`, `listen`, sets non-blocking.
- `before_step()`:
  - If no client is connected, briefly `select()` on the listening
    socket (50ms timeout). If readable, `accept()`.
  - If a client is connected, drain whatever bytes are available
    (non-blocking `recv()` loop until EAGAIN or peer closed).
  - Return `{has_conn, received}` snapshot. `received` is the full
    accumulated buffer for the current connection.
- `after_step(bindings)`:
  - If `out` is non-empty, `sendall()` it to the client.
  - If `close` is true, close the client socket.
- `stop()` closes both the client socket (if any) and the listening
  socket.

The plugin holds the only reference to socket file descriptors. Evident
never sees a fd or a partial recv — only the accumulated buffer and
whether someone is connected.

---

## Bytes Through a String

TCP carries bytes; Evident has no Bytes type. The plugin uses
`iso-8859-1` (latin-1) to round-trip arbitrary bytes through Evident's
`String` type losslessly. Each Python byte 0x00..0xFF maps to a single
Unicode codepoint and back. This works for HTTP (which is normally
ASCII-with-binary-bodies); for actual binary protocols you'd want to be
careful that the operations Evident performs on the string don't
interpret the bytes as text.

`\r` and `\n` in Evident string literals become real CR/LF bytes — the
parser already supports `\r`, `\n`, `\t`, `\\`, `\"` escapes in
`StringLiteral`. So the HTTP demo's `"HTTP/1.0 200 OK\r\n..."` is the
right 18 bytes after `1.0`.

---

## The Demo: HTTP in Evident

`programs/http_demo/server.ev` — about 12 lines of constraints:

```evident
import "stdlib/tcp.ev"

type main
    sock ∈ TCPSocket

    sock.has_conn ∧ sock.received contains "\r\n\r\n" ⇒ (
        sock.out   = "HTTP/1.0 200 OK\r\nContent-Length: 19\r\nConnection: close\r\n\r\nHello from Evident\n" ∧
        sock.close = true
    )

    ¬(sock.has_conn ∧ sock.received contains "\r\n\r\n") ⇒ (
        sock.out   = "" ∧
        sock.close = false
    )
```

The protocol is the program. Wait until `\r\n\r\n` appears in the
buffer (end of HTTP headers), then send the response and close. Until
then, send nothing and stay connected. This is what an HTTP/1.0 server
*does*, expressed as one if/else over socket state.

Tested with `curl -v http://localhost:PORT/` — returns 200 OK with the
expected body. Three pytest cases in `tests/conformance/test_tcp_server.py`
spawn the demo and hit it via urllib.

---

## What Comes Next

The natural follow-on is `Seq(TCPSocket)` or some equivalent multi-stream
type — the version where Evident sees N concurrent connections at once
and constraints describe per-connection state transitions. That's the
proxy / gateway / load-balancer story from the synchronous-reactive
design doc. It needs richer Set/Seq operations than we have today
(filtering by predicate, removing closed connections cleanly), so it's
deferred. The single-socket primitive proves the plumbing works.

What this design *doesn't* do — and explicitly shouldn't — is encode any
protocol knowledge in the plugin. No HTTP types. No headers as a first-class
concept. No content-length-aware framing. All of that is, properly,
the constraint program's job.
