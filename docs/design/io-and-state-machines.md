# I/O, State Machines, and the Evident Runtime

## The OS Observation

Sockets are not a language problem. Every language — C, Python, Haskell, Rust —
solves sockets the same way: syscalls. `socket()`, `bind()`, `listen()`,
`accept()`, `read()`, `write()`. The OS provides the interface. The language
just needs a way to call it.

The Evident "I/O problem" is therefore not "design an I/O model" — it is "expose
OS syscalls to Evident programs." This is an FFI problem, not an architecture
problem. The same answer that C gives (call the kernel) is the right answer for
Evident.

File I/O and socket I/O are the same problem: both are file descriptors (FDs).
STDIN is FD 0, STDOUT is FD 1, STDERR is FD 2. A socket is an FD. A file is
an FD. `read(fd, buf, n)` and `write(fd, buf, n)` work on all of them.

---

## STDIN/STDOUT in `evident run`

The `run` subcommand already executes `?` queries and prints results. That's
half of a stdio program: output. The other half — reading from stdin — is
missing.

A program that communicates via stdin/stdout looks like this in every language:

```
while True:
    line = read_line(stdin)
    result = process(line)
    write(stdout, result)
```

For Evident's `run` to support this, the runtime needs:
1. A way to read a line (or bytes) from stdin and bind it as a variable
2. A way to assert that value into the constraint system
3. A way to write query results to stdout (already works)
4. A loop that repeats until EOF

The key question: is this loop expressed in Evident, or is it the runtime's
responsibility? One option: the `run` command reads from stdin line-by-line,
asserts each line as a fact, re-runs the `?` queries, and writes results. The
Evident program declares what to do with each line without managing the loop.

---

## State Machines as Constraints

State machines are a natural fit for stream processing. A state machine is:
- A set of states (enum)
- A current state
- Transitions: (current_state, input) → (next_state, output)
- A start state and terminal states

This maps directly to Evident:

```
type LexerState = Start | InWord | InNumber | InSpace | Done | Error

schema Transition
    current ∈ LexerState
    byte    ∈ Nat            -- ASCII value of input byte
    next    ∈ LexerState
    output  ∈ String         -- what to emit, if anything

    -- Transitions
    current = Start   ∧ byte ≥ 65 ∧ byte ≤ 122 ⇒ next = InWord
    current = InWord  ∧ byte ≥ 65 ∧ byte ≤ 122 ⇒ next = InWord
    current = InWord  ∧ byte = 32               ⇒ next = InSpace
    current = InSpace ∧ byte = 32               ⇒ next = InSpace
    current = InSpace ∧ byte ≥ 65 ∧ byte ≤ 122 ⇒ next = InWord
    -- etc.
```

The solver can:
- Verify that a specific byte sequence takes the machine from Start to Done
- Find byte sequences that reach a desired terminal state
- Given a current state, determine what inputs lead to each possible next state
- Run the machine forward step-by-step

The forward-running case is what you'd need for real I/O: assert the current
state, assert the next input byte, query for the next state and output. The
runtime reads the byte and drives the query.

---

## State Machines and I/O — The Connection

A byte stream (stdin, socket, file) is a sequence of inputs to a state machine.
Processing a stream = driving a state machine one input at a time.

In Evident's execution model:
1. Start: `assert current_state = Start`
2. Read byte from FD: this is a syscall the runtime makes
3. Assert: `assert input_byte = 72`  (whatever byte was read)
4. Query: `? Transition` → solver returns `next_state = InWord`, `output = ""`
5. Assert: `assert current_state = next_state`
6. Go to 2

The Evident program declares the state machine. The runtime drives it by feeding
it bytes from the OS. The "loop" is the runtime, not the language.

This is the same model as reactive miniKanren, Erlang's gen_statem, and Haskell's
conduit streaming library — but expressed as constraint satisfaction rather than
code.

---

## What Would a POSIX FFI Look Like?

The minimal set of syscalls for useful programs:

```
-- File/FD operations
open(path ∈ String, flags ∈ Nat) → fd ∈ Nat
read(fd ∈ Nat, n ∈ Nat) → bytes ∈ String   -- simplified
write(fd ∈ Nat, bytes ∈ String) → written ∈ Nat
close(fd ∈ Nat)

-- Socket operations (all FD-based)
socket(domain ∈ Nat, type ∈ Nat) → fd ∈ Nat
bind(fd ∈ Nat, port ∈ Nat)
listen(fd ∈ Nat, backlog ∈ Nat)
accept(fd ∈ Nat) → client_fd ∈ Nat
connect(fd ∈ Nat, host ∈ String, port ∈ Nat)

-- Multiplexing (for handling multiple FDs)
select(read_fds ∈ Set Nat, timeout ∈ Nat) → ready_fds ∈ Set Nat
```

These could be exposed as special claims whose satisfaction triggers a syscall
in the runtime, binding the return value. They are effectful and cannot be
undone — which is the `action` concept from the web-server exploration doc.

---

## Open Questions

1. **Who controls the loop?** Is the event loop in the Evident program (as some
   kind of recursive or iterative construct) or in the runtime (which feeds the
   Evident program one event at a time)? The latter is simpler to implement and
   consistent with how most reactive frameworks work.

2. **How do you express "read the next byte and do something with it"?** This
   requires either a continuation-passing model or explicit state management.
   The state machine schema above handles this naturally: the transition is
   declared, the runtime drives it.

3. **Can state machines be recursive?** An HTTP parser is a hierarchy of state
   machines (line parser inside request parser). Can an Evident state machine
   schema call another? Schema composition via `..` might handle this naturally.

4. **What is the execution unit?** In Erlang it's a process. In Node.js it's a
   callback. In Go it's a goroutine. For Evident, the equivalent might be a
   single query evaluation — stateless, pure, parallelizable. The runtime handles
   concurrency by running many queries in parallel, each with its own evidence
   snapshot.

5. **How does backpressure work?** When reading from a slow client, the runtime
   should not spin. Some way to yield / block / wait is needed. This might just
   be "the runtime blocks on the syscall and Evident doesn't know about it."
