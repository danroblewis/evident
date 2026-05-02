# I/O as Evident Traits

Unix I/O resources — files, stdin, sockets, pipes — share a common foundation:
they are all file descriptors. They differ in which operations they support.
This document models them as Evident schemas composed from shared traits using
the existing `..` passthrough mechanism.

The `..` passthrough is already trait composition. A schema that includes
`..Readable` inherits both the variables and the constraints of `Readable`.
The solver enforces trait invariants automatically everywhere the trait appears.

---

## Base Traits

### Descriptor

Every Unix I/O resource is a file descriptor.

```
schema Descriptor
    fd       ∈ Nat
    open     ∈ Bool
    blocking ∈ Bool

    fd ≤ 1024       -- practical upper bound
    ¬open ⇒ blocking = false
```

### Readable

Anything you can read bytes from.

```
schema Readable
    ..Descriptor
    available ∈ Nat    -- bytes currently in kernel read buffer
    eof       ∈ Bool

    eof ⇒ available = 0
    ¬open ⇒ eof
```

### Writable

Anything you can write bytes to.

```
schema Writable
    ..Descriptor
    send_buffer ∈ Nat  -- bytes in kernel send/write buffer

    ¬open ⇒ send_buffer = 0
```

### Seekable

Anything with a position that can be moved. Only regular files support this.

```
schema Seekable
    ..Readable
    position ∈ Nat
    size     ∈ Nat

    position ≤ size
    eof ⇔ position = size
    available = size - position
```

### Buffered

User-space buffering (libc FILE* layer) on top of a descriptor.

```
schema Buffered
    ..Descriptor
    buffer_size ∈ Nat
    buffered    ∈ Nat
    flushed     ∈ Bool

    buffered ≤ buffer_size
    flushed ⇒ buffered = 0
```

### Connectable

Resources with explicit connection state: sockets.

```
type ConnectionState = Unconnected | Connecting | Connected | Listening | Closed

schema Connectable
    ..Descriptor
    connection ∈ ConnectionState

    connection = Closed    ⇒ ¬open
    connection = Connected ⇒ open
    connection = Listening ⇒ open
```

---

## Composed Types

### Stdin

```
schema Stdin
    ..Readable
    fd = 0
```

### Stdout

```
schema Stdout
    ..Writable
    ..Buffered
    fd = 1
    flushed ⇒ send_buffer = 0
```

### Stderr

Unbuffered by default — writes go directly to the kernel.

```
schema Stderr
    ..Writable
    fd = 2
    buffer_size = 0
```

### Regular File

```
type OpenMode = ReadOnly | WriteOnly | ReadWrite | Append

schema RegularFile
    ..Seekable
    ..Writable
    ..Buffered
    path     ∈ String
    mode     ∈ OpenMode

    path ⊑ "/"               -- absolute path
    mode = ReadOnly ⇒ send_buffer = 0
    mode = Append   ⇒ position = size  -- writes always go to end
```

### Pipe

A pipe has two ends. Each end is a separate descriptor.

```
schema Pipe
    reader       ∈ Readable
    writer       ∈ Writable
    write_closed ∈ Bool

    -- When the write end is closed and the buffer is drained, EOF
    write_closed ∧ reader.available = 0 ⇒ reader.eof

    -- Writer closing doesn't immediately cause EOF — buffered bytes remain
    ¬write_closed ⇒ ¬reader.eof
```

### Unix Domain Socket

```
schema UnixSocket
    ..Readable
    ..Writable
    ..Connectable
    path ∈ String   -- socket file path

    connection = Connected   ⇒ available ≥ 0
    connection = Unconnected ⇒ available = 0
    connection = Listening   ⇒ available = 0
```

### TCP Socket

```
schema TcpSocket
    ..Readable
    ..Writable
    ..Connectable
    remote_addr ∈ String   -- "address:port"
    local_port  ∈ Nat

    1 ≤ local_port ≤ 65535
    connection = Listening  ⇒ available = 0
    connection = Connecting ⇒ available = 0
    connection = Connected  ⇒ open
```

### Terminal (TTY)

```
type TerminalMode = Canonical | Raw | CBreak

schema Terminal
    ..Readable
    ..Writable
    mode ∈ TerminalMode

    -- Terminals do not EOF on normal use (Ctrl+D sends an EOF signal,
    -- but the terminal itself remains open)
    eof = false

    -- Canonical mode buffers until newline; raw mode makes each
    -- keystroke immediately available
    mode = Canonical ⇒ blocking = true
```

---

## Operations as Transition Schemas

Operations are schemas that describe transitions from one resource state to
the next. The runtime implements each transition with the actual syscall.
The schema is the specification; the syscall is the implementation.

### ReadBytes

```
schema ReadBytes
    src      ∈ Readable
    src_next ∈ Readable
    count    ∈ Nat         -- requested byte count
    data     ∈ Seq(Nat)    -- bytes read (each 0..255)
    n_read   ∈ Nat

    ∀ i ∈ {0..#data-1}: 0 ≤ data[i] ≤ 255

    n_read = #data
    n_read ≤ count
    n_read ≤ src.available

    -- Blocking read always returns at least one byte unless at EOF
    src.blocking ∧ ¬src.eof ⇒ n_read > 0

    src.eof ⇒ n_read = 0

    src_next.available = src.available - n_read
    src_next.eof       = src.eof ∧ src_next.available = 0
    src_next.fd        = src.fd
    src_next.blocking  = src.blocking
    src_next.open      = src.open
```

### WriteBytes

```
schema WriteBytes
    dst       ∈ Writable
    dst_next  ∈ Writable
    data      ∈ Seq(Nat)
    n_written ∈ Nat

    ∀ i ∈ {0..#data-1}: 0 ≤ data[i] ≤ 255

    n_written ≤ #data
    dst.open  ⇒ n_written > 0
    ¬dst.open ⇒ n_written = 0

    dst_next.send_buffer = dst.send_buffer + n_written
    dst_next.fd          = dst.fd
    dst_next.open        = dst.open
    dst_next.blocking    = dst.blocking
```

### Seek

```
type SeekMode = FromStart | FromCurrent | FromEnd

schema Seek
    file      ∈ Seekable
    file_next ∈ Seekable
    offset    ∈ Nat
    whence    ∈ SeekMode

    whence = FromStart   ⇒ file_next.position = offset
    whence = FromCurrent ⇒ file_next.position = file.position + offset
    whence = FromEnd     ⇒ file_next.position = file.size - offset

    file_next.position ≤ file_next.size
    file_next.size     = file.size
    file_next.fd       = file.fd
    file_next.open     = file.open
```

### Accept (TCP server)

```
schema Accept
    listener      ∈ TcpSocket
    listener_next ∈ TcpSocket
    new_conn      ∈ TcpSocket

    listener.connection = Listening
    listener_next.connection  = Listening
    listener_next.local_port  = listener.local_port

    new_conn.connection  = Connected
    new_conn.local_port  = listener.local_port
    new_conn.available   = 0
    new_conn.open        = true
```

### Connect (TCP client)

```
schema Connect
    sock      ∈ TcpSocket
    sock_next ∈ TcpSocket
    addr      ∈ String

    sock.connection = Unconnected
    sock_next.connection  = Connected
    sock_next.remote_addr = addr
    sock_next.local_port  = sock.local_port
    sock_next.fd          = sock.fd
```

---

## Trait Composition Summary

| Resource | Traits |
|---|---|
| `Stdin` | `Readable` |
| `Stdout` | `Writable`, `Buffered` |
| `Stderr` | `Writable` (unbuffered) |
| `RegularFile` | `Seekable` (extends `Readable`), `Writable`, `Buffered` |
| `Pipe.reader` | `Readable` |
| `Pipe.writer` | `Writable` |
| `UnixSocket` | `Readable`, `Writable`, `Connectable` |
| `TcpSocket` | `Readable`, `Writable`, `Connectable` |
| `Terminal` | `Readable`, `Writable` |

---

## What This Means for Evident

**Generic schemas.** A step schema that takes `∈ Readable` works with stdin,
files, sockets, and pipes without modification. This is Rust's `T: Read` or
Haskell's `Handle` — expressed as constraint composition.

**Trait invariants are enforced.** When a schema includes `..Readable`, the
solver verifies `eof ⇒ available = 0` automatically. Violating a trait
invariant makes a transition unsatisfiable — the solver catches it before
the syscall runs.

**Syscall specifications.** The operation schemas (`ReadBytes`, `WriteBytes`,
`Seek`) are formal specifications of what syscalls do. The runtime implements
them; the schemas verify that usage is consistent with the spec. A schema
that tries to read from a closed fd will be UNSAT before any syscall fires.

**`schema main` as the syscall binding site.** Only `main` introduces concrete
I/O resources (a `Stdin`, a `TcpSocket`) and binds their transitions to actual
syscall implementations. Sub-schemas work with trait types (`Readable`,
`Writable`) and are therefore testable without real I/O — pass a mock `Readable`
schema, the constraints still hold.
