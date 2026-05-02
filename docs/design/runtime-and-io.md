# Runtime and I/O: The Execution Model

## The Central Separation

Evident constraint schemas are pure mathematical relations. They describe *what
is true* — valid combinations of variable values. They have no awareness of time,
sequence, or the outside world.

The runtime is the execution engine that gives schemas life. It:
- Manages external data sources and sinks
- Decides when to trigger a solve
- Threads state between solves
- Handles all I/O mechanics

The programmer writes only the relation. The runtime handles everything else.
This is the core design principle: remove programmers from responsibility for
imperative "do-er" operations — reading bytes, writing output, managing memory,
advancing counters — by making those entirely the runtime's concern.

---

## The Three Modes of Variable Binding

Currently Evident populates variables in two ways:

**1. Given (explicit):** the caller provides a value before solving.
`rt.query('Schema', given={'x': 5})` pins `x = 5`; the solver fills in
everything else.

**2. Free / sampled:** no value is provided. The solver finds any satisfying
value, or the sampler assigns a random one for exploration.

For streaming programs, a third mode is needed:

**3. Context-bound:** the variable receives its value from a named external
context that the runtime manages. From the constraint system's perspective, the
variable looks identical to a given — it arrives with a concrete value before
solving begins. The *source* of that value is entirely the runtime's concern.

Context-bound variables make I/O invisible to the schema body. The schema
declares a constraint like `char ∈ Char`; the runtime decides that `char` comes
from stdin and provides the value accordingly.

---

## Contexts

A *context* is a named external data source or sink managed by the runtime.
Concrete examples:

| Context | Direction | What it provides / accepts |
|---|---|---|
| `stdin` | source | bytes from standard input |
| `stdout` | sink | bytes to standard output |
| `file("path")` | source or sink | bytes from/to a file |
| `socket(addr)` | bidirectional | bytes over a network connection |
| Another schema | source | outputs from a sibling schema's solve |

Contexts are not sets in the mathematical sense. `stdin` is not the set of all
possible bytes — it is a specific, ordered, stateful stream with a current
position and an eventual EOF. The constraint system never sees the context
directly. It only sees the values the runtime extracts from it.

**Context binding** connects a variable to a context:
- Source binding: before each solve, the runtime reads from the context and
  provides the value as the variable's given.
- Sink binding: after each solve, the runtime reads the variable's value from
  the model and writes it to the context.

The granularity of what is read per step — one byte, one chunk, one line — is
a property of the context configuration, not of the schema.

---

## When Does a Solve Happen?

A solve is triggered when all context-bound *source* variables have received
their values for the current step. This is the natural completion condition:
the constraint system has all the inputs it needs, so it can run.

For a char-at-a-time program: one char arrives from stdin → the only source
variable is ready → solve immediately. For a chunk-based program: the runtime
reads however many bytes `read()` returns, provides the chunk as one variable,
then solves. The schema is agnostic to read granularity.

This "inputs are ready" trigger is an event model: an event is "a unit of
external input has arrived." Each event triggers one solve. The solve produces
outputs (written to sinks) and new state (carried to the next step). Then the
runtime waits for the next event.

The event model is more general than the stream model. A stream is a special
case where events arrive continuously from a single source. A program could also
respond to multiple sources — a byte from stdin AND a timer — where the solve
fires when any source has new data, or when all sources have new data, depending
on configuration.

---

## Carried State

After each solve, some output variables persist as given for the *next* solve.
These are *carried variables* — the memory of the system between steps.

The runtime stores carried variables. The programmer declares:
- Which variables carry forward
- What their initial values are (before the first solve)
- Which output variable "replaces" which input variable (e.g. `n_next` replaces
  `n`)

Carried variables are the only persistent state in an Evident program. The
runtime allocates and manages their memory. The programmer never writes
allocation, deallocation, or mutation — carried state is just "the output of
this solve becomes the input of the next."

For `nl`:
```
Carried:  n (Nat, init 1), partial (String, init "")
Source:   char (from stdin, one byte per step)
Sink:     out (to stdout, only when out ≠ "")
```

The schema body is purely the transition function:
- If `char = "\n"`: emit `int_to_str n ++ "\t" ++ partial`, reset `partial`,
  increment `n`
- Otherwise: append `char` to `partial`, leave `n` unchanged

No `fread`, no `printf`, no `n++`. The runtime handles all of that based on
the context configuration.

---

## The Runtime as a Universal Executor

The runtime for I/O programs is not specific to any application. It is a
generic state machine executor:

```
loop:
    1. Read from each source context → populate source variables
    2. Merge with carried variables → build this step's given
    3. Solve the schema with given
    4. Extract model values
    5. Write sink variables to their sink contexts
    6. Store carried variables for next step
    7. If EOF or terminal condition: stop. Otherwise: goto 1.
```

This loop is the same for `nl`, for `grep`, for any streaming Evident program.
The only things that vary are: which schema, which contexts, which variables are
carried, and the initial state. These are configuration, not computation.

The programmer writes only the schema. The runtime drives execution. This is
a complete separation of *what* from *how*.

---

## EOF and Termination

EOF is a special event — the source context signals that no more data is coming.
The runtime needs to handle it. Options:

**Ignore and stop:** when EOF arrives, stop the loop. Simple, but misses the
"flush" case — a partial line in `nl` that has no trailing newline.

**Terminal solve:** when EOF arrives, trigger one final solve with a special
`eof = true` variable set. The schema can have a clause that fires on EOF to
handle remaining state (flush the partial buffer, emit a final record, etc.).

**EOF as a value:** `eof ∈ Bool` is a source variable that is `false` for every
normal step and `true` for the final step. The schema handles both cases as
regular constraint clauses.

The terminal solve approach is cleanest — it doesn't require the runtime to
know anything special about "flushing," and it lets the schema handle its own
termination logic as a regular constraint.

---

## What Remains to Design

This model is architecturally settled. What hasn't been decided is syntax:

**How does the programmer declare the wiring?**

The binding between variables and contexts, the carried variables, the initial
state, the entry schema — all of this needs to be expressed somewhere. Options:

1. A special `schema main` with new wiring declarations in the body
2. Annotations on regular schemas (`-- @source`, `-- @carry`, etc.)
3. A separate configuration file or command-line flags
4. A new top-level block type distinct from `schema`

The constraint schemas themselves need no new syntax. The wiring is configuration
— the question is where that configuration lives and what it looks like.

The guiding principle from Evident's existing design: if it can look like a
constraint, make it look like a constraint. `char ∈ Stdin` reads naturally
as "char comes from Stdin" — using the existing membership notation — even
though Stdin is a context, not a set. This reuse reduces the number of new
concepts the programmer needs to learn.
