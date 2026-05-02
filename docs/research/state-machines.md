# State Machines and Constraint Systems

## The Core Insight

A state machine's transition function is a relation:

```
δ ⊆ S × Σ × S
```

where S is the set of states, Σ is the input alphabet, and (s, a, s') ∈ δ means
"from state s, on input a, the machine may transition to s'". This is already the
central abstraction of Evident — a named set of tuples, queried by membership
constraints. The state machine and the constraint system are the same thing viewed
from different angles.

---

## The Taxonomy of State Machine Models

### Finite State Machines (FSMs)

The basic model: **(S, Σ, δ, s₀, F)** — states, alphabet, transition relation,
initial state, accepting states. The transition relation δ may be a function
(deterministic, DFSM) or a full relation (nondeterministic, NFSM).

Nondeterminism is natural for constraint solving: multiple valid next states
correspond to multiple satisfying assignments, which the solver is already
designed to find.

### Moore Machines vs. Mealy Machines

Both are FSMs extended with output — transducers that map input sequences to
output sequences.

**Moore machine:** output depends only on the current state.
- λ: S → Δ (output function over states)
- Produces one output per state entered
- Output is stable; doesn't depend on what input triggered the state

**Mealy machine:** output depends on the current state AND the current input.
- λ: S × Σ → Δ (output function over transitions)
- Produces one output per transition fired
- More compact — generally fewer states needed than the equivalent Moore machine

**For stream processing, Mealy is the right model.** When processing a character
stream, each input character triggers a transition and possibly an output. The
`nl` program is a Mealy transducer: on each character, it either accumulates
(no output) or emits a numbered line (output depends on both the accumulated
partial line and the newline character that triggered it).

### Statecharts (Harel, 1987)

Harel's statecharts extend FSMs with three features that address state explosion:

**1. Hierarchy.** States can contain substates. Transitions at the parent level
apply to all child states simultaneously, eliminating duplication.

**2. Orthogonal regions.** A state can have multiple independent concurrent
sub-machines running in parallel. Both regions are active when the parent state
is active.

**3. History states.** When re-entering a compound state, optionally restore the
last-active sub-state (shallow history) or the deepest last-active sub-state
(deep history).

Statecharts also add **entry and exit actions** (Moore-style effects triggered on
state entry/exit) and **guard conditions** (predicates that must be satisfied for
a transition to fire).

Guard conditions are already expressible in Evident as constraints. Entry/exit
actions are the novel addition — effects that happen as a consequence of a
state transition.

### Register Machines

An FSM extended with an unbounded set of addressable memory cells (registers).
Operations: increment, decrement, conditional branch, loop. Turing-complete.

The register machine is relevant because Evident's "carried variables" are
registers: named memory cells whose values persist between state machine steps,
managed by the runtime without programmer intervention.

### Petri Nets

A bipartite directed graph of places (representing state) and transitions
(representing events). Tokens at places represent the current state; firing a
transition moves tokens. Petri nets naturally model concurrent and distributed
state machines.

Relevant for Evident because:
- Multiple tokens at multiple places = multiple simultaneous constraint variables
- Transition firing = a constraint solve step
- Concurrent regions in statecharts = orthogonal Petri net sub-nets

---

## State Machine Properties

### Safety and Liveness

These are the two fundamental classes of temporal property, proven complete by
Alpern and Schneider (1985):

**Safety properties:** "Nothing bad ever happens."
- A trace violates a safety property if it contains a finite bad prefix.
- Examples: mutual exclusion, no invalid state, no constraint violation.
- Verifiable by examining finite executions.

**Liveness properties:** "Something good eventually happens."
- A trace violates a liveness property only in the limit — no finite prefix is
  bad by itself.
- Examples: termination, eventual constraint satisfaction, eventual output.
- Require reasoning about infinite executions.

**Key theorem:** every temporal property is a conjunction of a safety and a
liveness property.

### Reachability

State s' is reachable from s₀ if there exists a finite sequence of transitions
from s₀ to s'. Reachability analysis identifies dead states (never reachable)
and unsafe states (reachable but should not be).

In constraint terms: reachability is the transitive closure of the transition
relation — expressible in Evident's set language as a fixpoint computation.

### Determinism and Completeness

- **Deterministic:** for every (state, input) pair, exactly one transition.
- **Complete:** for every (state, input) pair, at least one transition (no dead
  inputs).
- **Total:** deterministic AND complete.

For stream processing, a total deterministic machine is required: every input
in every state must produce exactly one next state. Nondeterminism means the
runtime must choose — either by backtracking (full search) or by oracle
(random choice).

### Deadlock Freedom

A state is a deadlock if no transition is enabled. Deadlock freedom is a safety
property (bad state never reached) and also a liveness concern (progress is
always possible).

In constraint terms: the schema must be satisfiable for every possible
(current_state, input) combination. The solver can check this.

---

## Declarative State Machines in Other Systems

### TLA+ (Temporal Logic of Actions)

Lamport's TLA+ specifies state machines as temporal logic formulas over
primed/unprimed variables. A complete system specification has the form:

```
Init ∧ □[Next]_vars
```

- `Init` is the initial condition predicate
- `Next` is the step relation — a predicate over unprimed (current) and primed
  (next) variables
- `□[Next]_vars` means "always, either Next holds or vars are unchanged"

Example:

```
VARIABLES state, partial, n

Init == state = "accumulating" ∧ partial = "" ∧ n = 1

Next == ∃ char ∈ Char :
    ∨ char = '\n' ∧ state = "accumulating" ∧
      output' = ToString(n) ∘ "\t" ∘ partial ∧
      partial' = "" ∧ n' = n + 1 ∧ state' = "accumulating"
    ∨ char ≠ '\n' ∧ state = "accumulating" ∧
      partial' = partial ∘ char ∧ n' = n ∧ state' = "accumulating"
```

The primed variable pattern (x' = x + 1) is exactly what Evident's "carried
state" concept would express. TLA+ has been used to formally verify AWS
DynamoDB, S3, and EBS.

### Erlang gen_statem

Erlang's gen_statem framework models state machines as a set of callback
functions — one per state (or one for all states). Each callback is a pure
function: `(event, data) → (actions, next_state, new_data)`.

The key properties:
- State is explicit data passed between callbacks
- Transitions return actions (effects to perform) as data
- The runtime (OTP) executes the actions
- No global mutable state — the programmer describes transitions, the runtime
  drives the machine

This is the separation Evident targets: the constraint schema describes the
transition logic; the runtime drives execution and manages effects.

### XState (JavaScript)

XState represents state machines as plain data objects — serializable, transmissible,
visualizable without execution. The machine definition is a constraint over valid
states, inputs, and transitions. The actual execution is handled by an interpreter
that drives the data.

The functional core: `machine.transition(state, event)` is a pure function.
This is the evaluator pattern Evident already implements for `rt.query(schema, given)`.

### Datalog

Datalog can express state machine transitions as facts and reachability as
recursive rules:

```datalog
transition(s0, a, s1).
transition(s1, b, s0).

reachable(S0) :- initial(S0).
reachable(S')  :- reachable(S), transition(S, _, S').
```

The fixpoint semantics of Datalog — repeatedly apply rules until no new facts
are derived — is exactly forward-chaining constraint propagation. Evident's
`ForwardRule` system is already Datalog.

---

## The Constraint System / State Machine Duality

A state machine step is a function:

```
step : State × Input → State × Output
```

A constraint schema is a relation:

```
schema Step
    state       ∈ State
    input       ∈ Input
    next_state  ∈ State
    output      ∈ Output
    -- body: constraints relating these four
```

These are the same structure. The schema body expresses the valid combinations
of (state, input, next_state, output). The solver finds satisfying assignments.
Running the state machine IS solving the constraint schema, one step at a time,
with:

- `state` and `input` provided as `given`
- `next_state` and `output` found by the solver
- `next_state` carried back as `state` for the next step

The programmer writes only the schema body. The runtime handles the step loop,
memory management, and I/O.

### Three Modes of Variable Population (Revisited)

Evident currently populates variables two ways:

| Mode | Mechanism | Who decides |
|---|---|---|
| Single solve | `given` — explicit values | caller |
| Sampling | random assignment | runtime |

State machines introduce a third:

| Mode | Mechanism | Who decides |
|---|---|---|
| Stream input | values arrive from outside | the world |

And a fourth variable role — carried state:

| Role | Mechanism | Who decides |
|---|---|---|
| Carried | value from this solve becomes `given` for next solve | runtime |

All four are ways the runtime manages variable binding. The programmer declares
which variables play which role. The runtime handles everything else.

### Mealy Machines as Evident Schemas

For the `nl` program:

```
-- State
partial : String    (accumulated partial line)
n       : Nat       (line counter)

-- Input (sourced from stdin one char at a time)
char    : Char

-- Output (sinked to stdout, empty when no complete line yet)
out     : String

-- Next state (carried)
partial_next : String
n_next       : Nat
```

The schema body is the transition function:

```
schema NlStep
    partial      ∈ String
    n            ∈ Nat
    char         ∈ String
    out          ∈ String
    partial_next ∈ String
    n_next       ∈ Nat

    char = "\n" ⇒ out          = int_to_str n ++ "\t" ++ partial
    char = "\n" ⇒ partial_next = ""
    char = "\n" ⇒ n_next       = n + 1

    char ≠ "\n" ⇒ out          = ""
    char ≠ "\n" ⇒ partial_next = partial ++ char
    char ≠ "\n" ⇒ n_next       = n
```

No imperative code. No `fread`, no `printf`, no `n++`. The programmer describes
what is true about the transition. The runtime reads `char` from stdin, carries
`partial_next` and `n_next` forward, and writes `out` to stdout (when non-empty).

---

## What Evident Would Need

### Already Present

- Named set membership: `(state, input, next_state) ∈ transitions` — the
  complete transition relation
- Guard conditions: `char = "\n" ⇒ ...` — conditional transitions
- String operations: `++`, `∋`, regex — sufficient for many stream processors
- Enum types: model discrete state sets
- Forward rules: Datalog-style chained implication

### Missing: Wiring Declarations

A way to declare which variables are sourced, sinked, and carried. The `schema main`
concept:

```
schema main
    run    NlStep
    source char         from stdin
    sink   out          to stdout  when out ≠ ""
    carry  partial      init ""
    carry  n            init 1
```

This is configuration, not logic. It belongs in a dedicated wiring schema.

### Missing: String Indexing

To find newline positions in a chunk of bytes (rather than reading one char at
a time), we need `index_of` and `sub_str`. Both are already in Z3 as `IndexOf`
and `SubString` — they just need to be exposed in Evident syntax.

### Missing: The Runtime Step Loop

The runtime needs to:
1. Read from sources (stdin — raw bytes, not line-buffered)
2. Feed sourced values as `given` to the step schema
3. Solve
4. Write sinked values to their sinks (stdout)
5. Store carried values for the next step
6. Repeat until the source is exhausted (EOF)

This loop is the only "do-er" code needed. It is generic — the same loop drives
any Evident state machine program. The programmer never writes it.

### Missing: Effects as First-Class Values (Optional — Later)

Statecharts include entry/exit actions and transition actions. For a pure
constraint language, effects are better treated as output variables that the
runtime interprets:

- `out ≠ ""` → write to stdout
- `emit_event = SomeEvent` → send to another state machine

This keeps schemas pure (no side effects in the body) while allowing the runtime
to produce effects.

---

## Temporal Logic and Verification (Future Direction)

Once state machines are expressible in Evident, temporal properties become
expressible as additional constraints:

**Safety** (bad state never reached):
```
∀ state ∈ reachable_states: state ≠ error_state
```

**Liveness** (good state eventually reached):
```
∀ trace: ∃ step ∈ trace: step.state = done
```

**Deadlock freedom** (always a valid transition):
```
∀ state ∈ reachable_states, input ∈ Σ:
    ∃ next ∈ State: (state, input, next) ∈ transitions
```

These are Evident constraints. The solver becomes a model checker.
Finite state spaces can be exhaustively verified. This is the long-term potential:
Evident programs are both executable AND formally verifiable, in the same language,
with the same solver.

---

## Summary

| Concept | State Machine Term | Evident Equivalent |
|---|---|---|
| State set | S | Enum type or schema |
| Input alphabet | Σ | Input enum or String/Char |
| Transition relation | δ ⊆ S × Σ × S | Named set + membership constraint |
| Guard condition | enabled(s, a) | Constraint in schema body (⇒) |
| Current state | s_current | Carried variable |
| Next state | s_next | Carried variable (next name) |
| Input | a | Sourced variable |
| Output | output | Sinked variable |
| Initial state | s₀ | `carry x init value` in schema main |
| Accepting state | F | Constraint that characterizes the terminal state |
| Mealy output function | λ(s, a) | Derived variable in schema body |
| Entry/exit action | effect on transition | Sinked variable with condition |
| History state | resume prior substate | Carried variable (no change needed) |
| Temporal property | □P, ◇P | ∀ constraint over reachable states |
