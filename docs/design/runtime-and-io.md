# Runtime and I/O: Evident as a Constraint Automaton Executor

## The Formal Model

After research into state machines, automata theory, and coordination models,
the right framing is now clear: **Evident programs are constraint automata.**

A constraint automaton is a tuple A = (Q, N, →, q₀) where:
- **Q** — a finite set of states (configurations of the system)
- **N** — a finite set of **ports** (named I/O endpoints)
- **→** — a transition relation: q -[P, D]→ q', where P ⊆ N is the set of
  ports that must synchronize, and D is a data constraint over values on those
  ports
- **q₀** — the initial state

A transition fires when all ports in P are simultaneously ready with data, and
the data satisfies constraint D. This is the formal mechanism for I/O in a
declarative system: not a special case, not a bolt-on, but the transition
relation itself.

Evident schemas are the data constraints D. The runtime is the automaton
executor. Stdin and stdout are ports.

---

## What Changed from the Previous Framing

The earlier version of this document introduced "contexts" as an ad-hoc concept
for external data sources and sinks. Constraint automata give us the precise
vocabulary that was missing:

| Previous term | Formal term | Meaning |
|---|---|---|
| Context | Port | Named I/O endpoint; part of the automaton's interface |
| Context-bound variable | Port variable | Variable whose value is delivered via a port |
| Source context | Input port | Port that delivers data into the automaton |
| Sink context | Output port | Port that receives data from the automaton |
| Carried state | Automaton state | Variable bindings that persist between transitions |
| Universal executor loop | Automaton execution | The standard step-fire-advance cycle |
| Schema composition | Product construction | Composition of automata at shared ports |

The concepts were right. The terminology is now grounded.

---

## Evident Schemas as Symbolic Automata

A further refinement from symbolic automata theory (Veanes et al., Microsoft
Research): Evident schemas are not just the data constraints D — they *are*
symbolic automaton predicates.

Classical automata label transitions with explicit alphabet symbols. Symbolic
automata label transitions with *predicates* over an alphabet theory, decided
by an SMT solver. Evident does exactly this:

- Transitions carry constraint predicates (`x > 5`, `s ∈ /[a-z]+/`, `n = 1`)
- The oracle that decides predicate satisfiability is Z3
- Composing schemas via `∧` is automaton intersection

The architecture of symbolic automata matches what Evident already does. The
runtime is already a symbolic automaton executor — it just hasn't been named
as such.

---

## Ports

A port is a named, directed endpoint for data flow. Ports are the mechanism
by which an Evident constraint automaton interacts with the world.

**Input ports** deliver data *into* the automaton before a transition fires.
**Output ports** receive data *from* the automaton after a transition fires.

Concrete ports:

| Port | Direction | Data |
|---|---|---|
| `stdin` | input | bytes or structured data from standard input |
| `stdout` | output | bytes or structured data to standard output |
| `file(path)` | input or output | data from/to a file |
| `socket(addr)` | bidirectional | data over a network connection |
| A sibling schema | bidirectional | shared port between composed automata |

A port is not a set. `stdin` is a specific, ordered, stateful data source with
a position and an eventual EOF signal. The constraint automaton never sees the
port directly — it only sees the data value the runtime delivers through it.

---

## States and Transitions

An Evident constraint automaton's **state** is its set of variable bindings
that persist between transitions — what was previously called "carried
variables." The runtime holds the state and provides it as `given` for each
solve.

A **transition** fires when:
1. All input ports in the synchronization set have data ready
2. The constraint schema is satisfiable with (state bindings) ∪ (port data) as
   given
3. The solver produces a model — new values for the state variables and output
   port variables

After a transition:
- New state variable values replace the old ones (state advance)
- Output port variable values are written to their ports
- The automaton waits for the next input

For the `nl` program, the constraint automaton is:

```
State:        n : Nat = 1, partial : String = ""
Input ports:  char : Char   (from stdin)
Output ports: out  : String (to stdout, when non-empty)

Transition constraint (schema body):
  char = "\n" ⇒ out = int_to_str n ++ "\t" ++ partial
  char = "\n" ⇒ partial_next = ""
  char = "\n" ⇒ n_next = n + 1
  char ≠ "\n" ⇒ out = ""
  char ≠ "\n" ⇒ partial_next = partial ++ char
  char ≠ "\n" ⇒ n_next = n
```

The automaton has one state (the current n and partial values) and one
transition that fires once per character from stdin. The runtime manages
the state; the schema body is the transition constraint.

---

## The Runtime as Constraint Automaton Executor

The runtime executes the standard constraint automaton step cycle:

```
Initialize: q₀ ← initial state (variable bindings at declared initial values)

Loop:
  1. Wait for all input ports to be ready (synchronization)
  2. Deliver port data + current state bindings to the solver as given
  3. Solve the transition constraint (the schema body)
  4. If UNSAT: transition is not enabled; report error or try alternatives
  5. Extract model values
  6. Write output port variables to their ports
  7. Advance state: new state variable values replace old ones
  8. If terminal condition (EOF, accepting state): stop
  9. Otherwise: goto 1
```

This loop is generic — it executes any Evident constraint automaton. The
only things that vary per program are the schema, the port declarations, the
initial state, and the terminal condition.

---

## Composition

Multiple schemas compose via **product construction** at shared ports.

Given two constraint automata A₁ and A₂:
- Their product A₁ ▷◁ A₂ has states Q₁ × Q₂
- Ports are the union N₁ ∪ N₂
- At shared ports (N₁ ∩ N₂), both automata must synchronize
- Data constraints are conjoined: D = D₁ ∧ D₂

Evident's existing `..SubSchema` passthrough is product construction where
the shared ports are the matching variable names. Field access like
`task.duration` is port sharing — the sub-automaton's output ports are the
parent's internal variables.

A sibling schema listed as a port is not a "context" — it is a composed
automaton. The runtime executes the product.

---

## Synchronization and the "When" Question

A transition fires when its synchronization set P is satisfied — when all
input ports in P have data ready simultaneously.

For a char-at-a-time program: P = {char} (one input port). One char arrives →
port is ready → transition fires. For a program with two input ports (e.g.
stdin and a config file): the transition fires when both ports have data.

This is the formal answer to the earlier question "when does a solve happen?"
A solve happens when the synchronization condition is met. The granularity of
what constitutes a "ready" input port — one byte, one chunk, one line — is a
property of the port configuration, not of the schema.

---

## EOF and Terminal States

EOF is the input port's signal that no more data is coming. In constraint
automaton terms, reaching EOF is reaching a state from which the only port in
the synchronization set has no more data — no transition is enabled.

Options for handling EOF:

**Terminal state:** the automaton has an explicit state that it transitions to
on EOF. The schema has a constraint clause matching an `eof = true` port
variable. Final output (flushing partial buffers, etc.) happens in the
transition to the terminal state.

**Implicit termination:** when no transition is enabled (EOF, or no satisfying
assignment), the runtime stops. Clean for simple programs; insufficient for
programs needing a flush step.

The terminal state approach is more general and matches how automata with
accepting states work. The `nl` program needs it: a partial line with no
trailing newline must be flushed when EOF arrives.

---

## What Remains to Design

The formal model is now clear. What remains:

**1. Port declaration syntax**

How does the programmer declare which variables bind to which ports, what the
initial state is, and what the terminal condition is? The constraint schemas
themselves need no new syntax. The port declarations are the open question.

Using Evident's existing membership notation: `char ∈ stdin` reads naturally
as "char is delivered by the stdin port." This reuses the existing `∈` syntax
and treats ports as named data sources — close to how the formal model works,
since a port delivers values from some domain.

**2. Accepting states / terminal conditions**

Classical automata have explicit accepting states F ⊆ Q. For streaming
programs, the terminal condition is usually EOF. But some programs terminate
on a constraint (e.g. "stop when n = 100"). Accepting state declarations
need syntax.

**3. Multi-port synchronization**

When a transition synchronizes on more than one input port, the runtime must
wait for all of them. How is the synchronization set P declared? Is it
implicit (all declared input ports) or explicit (named in the schema)?

**4. Symbolic automata and the two-level execution model**

The symbolic automata research suggests compiling schemas to automaton
structure once, then querying Z3 per transition (rather than rebuilding the
constraint system each step). This is a runtime optimization that follows
from the formal model.
