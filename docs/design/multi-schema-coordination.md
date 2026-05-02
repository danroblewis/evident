# Multi-Schema Coordination: Options and Decision Framework

## The Problem

Evident schemas are pure constraint systems. A single schema solves one focused
problem: given some variable bindings, find a satisfying assignment. The open
question is how multiple schemas coordinate — how outputs of one become inputs
to another, whether feedback cycles are allowed, and what drives the overall
execution.

This document collects the options surfaced by the research phase and frames
the decision. We are not ready to choose yet. The open questions at the end
explain why.

---

## Background: What the Research Found

Six research areas are directly relevant:

| Research | Key finding |
|---|---|
| **CHR** | Rules fire when constraints are added; chaining creates propagation networks; multi-headed rules match across multiple constraint stores |
| **Belief propagation** | Ping-pong between nodes converges to a fixpoint; Knaster-Tarski guarantees convergence when iteration is monotone |
| **Blackboard** | Shared state + specialist solvers + controller is a proven coordination architecture; control complexity is the hard part |
| **Dataflow/reactive** | Schemas as nodes in a graph; token flow drives execution; SDF gives static scheduling, KPN gives determinism |
| **Nelson-Oppen** | Theory combination via equality sharing is exactly what we want at the schema level; Z3 already does this internally |
| **Monads** | Evident's current pipeline already has Reader/Writer/State/Maybe structure implicitly; sequential composition is already sound |

---

## The Options

### Option 1: Sequential Pipeline Only

Schemas compose in one direction: A → B → C. The output of A becomes the given
for B, B's output becomes the given for C. No feedback. No cycles.

This is close to what Evident already does with `..SubSchema` passthrough.
Extending it to runtime contexts (stdin → schema → stdout) and carried state
gives us the streaming I/O model from the runtime design doc, without any new
coordination mechanism.

**Pros:**
- Already partially implemented
- Deterministic — same input always produces same output
- Simple to reason about, simple to implement
- Monadic structure (the research confirmed this): sequential composition has
  algebraic guarantees
- Sufficient for most streaming programs (nl, grep, transform pipelines)

**Cons:**
- Cannot express feedback: schema B cannot influence schema A
- Cannot model iterative refinement
- Cannot model mutual dependencies between schemas
- One-pass only — some problems require multiple iterations to solve

**When it's enough:** stream processors, ETL pipelines, sequential derivation
chains, any program where data flows in one direction.

---

### Option 2: CHR-Style Propagation Rules Between Schemas

Extend Evident's existing forward rule system (`A, B ⇒ C`) to fire across
schema boundaries. When a schema produces a binding, that binding is available
to any rule that matches it, potentially triggering other schemas or adding new
constraints.

CHR has three rule types:
- **Propagation:** add new constraints without removing existing ones
- **Simplification:** replace constraints with simpler equivalents
- **Simpagation:** remove some constraints, add others

For Evident, the inter-schema version would be: when schema A produces a binding
for variable X, any rule that mentions X can fire and populate variables in
schema B.

**Pros:**
- Natural extension of Evident's existing forward rules
- Well-studied: confluence and termination theory from CHR applies directly
- Supports incremental constraint addition (important for streaming)
- Explicit propagation path — each step is traceable

**Cons:**
- Confluence (does order of rule firing matter?) must be verified or guaranteed
- Termination is not guaranteed without monotonicity
- Adds significant complexity to the runtime
- Multi-headed rules (matching across multiple schemas simultaneously) are
  expensive

**When it's appropriate:** systems with many interacting constraints, type
inference, planning, any domain where new information triggers cascading updates.

---

### Option 3: Blackboard Architecture

Schemas become knowledge sources. The runtime maintains a shared state (the
blackboard) containing all current variable bindings across all active schemas.
A controller/scheduler decides which schema to activate based on what data is
available.

When a schema runs, it reads from the blackboard and writes new bindings back.
Other schemas waiting for those bindings become eligible to run. The process
continues until no schema has enough input to fire, or until a terminal
condition is met.

**Pros:**
- Naturally opportunistic: schemas fire when their data is ready, not on a fixed
  schedule
- Extensible: adding a new schema adds a new specialist without touching others
- Good theoretical foundation from AI systems (HEARSAY-II, BB1)
- Control can be made explicit: the scheduler's decisions are themselves
  observable and tunable
- Supports feedback: schema B can write to the blackboard and schema A can read
  those writes on a subsequent activation

**Cons:**
- Control complexity is the hard problem — what fires next?
- Non-deterministic execution order (unless the scheduler is deterministic)
- Shared mutable state requires careful coordination
- Debugging is harder: why did this schema fire before that one?
- The controller is itself a complex component

**When it's appropriate:** exploratory or search-driven problems, expert systems,
any domain where the order of sub-problem solving is data-driven and hard to
predict in advance.

---

### Option 4: Explicit Dataflow Graph

The programmer declares the topology: which schemas feed which, which variables
flow along which edges. The runtime drives execution according to the declared
graph.

The topology could be expressed as new `schema main` wiring declarations, or
as a separate configuration, or as annotations. The runtime then executes
according to dataflow semantics — a schema fires when all its input edges have
data, sends outputs on its output edges.

Two sub-variants from the research:
- **Synchronous dataflow (SDF):** fixed number of tokens per firing, static
  schedule computable at compile time, deterministic.
- **Kahn process networks:** blocking reads, non-blocking writes, deterministic
  but schedule is dynamic.

**Pros:**
- Programmer has full control over the topology
- Can support cycles (feedback arcs) explicitly
- SDF variant enables static analysis and scheduling
- Visual: the topology IS the program structure, naturally diagrammable
- Well-studied: Ptolemy project has 30+ years of research on this

**Cons:**
- Requires programmers to explicitly declare the graph — more code to write
- Topology declaration is new syntax (yet to be designed)
- Buffering and deadlock prevention become programmer concerns (partially)
- SDF's fixed token rates may be too rigid for constraint schemas with variable
  output sizes

**When it's appropriate:** signal processing, systems where the data flow
topology is known and stable, programs that benefit from static scheduling
analysis.

---

### Option 5: Nelson-Oppen Style — Schemas as Theories

The most theoretically principled option. Each schema is treated as a decision
procedure for a sub-theory. The runtime coordinates them using equality sharing:
when schema A learns that variable X = 5, it shares that equality with any
schema that also mentions X. Those schemas propagate it and may learn new
equalities, which are shared back.

This is exactly what Z3 does internally between its arithmetic, string, and
array theory solvers. We would be exposing that pattern at the language level.

**Pros:**
- Formal correctness guarantees: if schemas have disjoint variable scopes and
  stably infinite constraints, combination is sound and complete
- Z3 already does this — we can leverage its infrastructure
- Matches the mathematical structure of constraint solving most faithfully
- Schema boundaries remain clean — each schema only knows about its own variables

**Cons:**
- Requires schemas to have (mostly) disjoint variable scopes, which conflicts
  with the composition model we already have
- Stably-infinite requirement rules out some useful constraint types
- Convexity analysis is complex — non-convex theories require case splitting,
  which multiplies the search space
- The equality-sharing protocol requires significant runtime machinery

**When it's appropriate:** formal verification contexts, systems requiring
correctness guarantees, combining well-defined sub-theories each with their own
solver.

---

### Option 6: Belief Propagation / Iterative Fixpoint

Schemas are nodes in a factor graph. Each node sends "messages" (partial
solutions, domain restrictions) to its neighbors. The runtime runs rounds of
message passing until the system reaches a fixpoint or a maximum iteration
count.

The Knaster-Tarski theorem guarantees that if each schema's computation is
monotone (outputs only grow — new information never invalidates old information),
a fixpoint exists and the iteration converges.

**Pros:**
- Handles cyclic dependencies gracefully
- Well-studied convergence theory
- Parallel-friendly: nodes can update independently
- Monotone schemas give free convergence guarantees

**Cons:**
- Non-monotone schemas (those using negation) do not have convergence guarantees
- Loopy message passing (cycles in the schema graph) may oscillate
- Convergence detection requires comparing successive iterates
- Iteration count may be large for complex systems

**When it's appropriate:** probabilistic or approximate reasoning, systems with
many weakly-coupled interacting schemas, any setting where "good enough after K
iterations" is acceptable.

---

## Comparison Summary

| Option | Feedback? | Cycles? | Deterministic? | Implementation complexity | Theory basis |
|---|---|---|---|---|---|
| 1. Sequential | No | No | Yes | Low (already exists) | Monad composition |
| 2. CHR rules | Yes | With care | Depends on rules | High | CHR theory |
| 3. Blackboard | Yes | Yes | Depends on scheduler | High | AI systems |
| 4. Dataflow graph | Yes (explicit) | Yes (explicit) | SDF: yes; KPN: yes | Medium | Dataflow theory |
| 5. Nelson-Oppen | Limited | No | Yes | High | SMT theory |
| 6. Belief propagation | Yes | Yes | No (iterative) | Medium | Probabilistic inference |

---

## What We Already Know Works

Option 1 (sequential pipeline) is already partially implemented and is
sufficient for streaming I/O programs. The runtime design doc (runtime-and-io.md)
describes how to extend it with context binding and carried state. This requires
no new coordination mechanism.

The question is whether Evident should support *more* than sequential composition,
and if so, which of Options 2–6 to adopt.

---

## Open Questions That Block the Choice

**1. What use cases require feedback?**
If all practical Evident programs can be expressed as sequential pipelines, there
is no need for Options 2–6. What is the simplest program that cannot be expressed
without feedback between schemas?

**2. Should cycles be allowed at all?**
Cyclic schema dependencies raise hard questions about termination. Is it better
to prohibit cycles entirely (as in Option 1 and 5) and require programmers to
make feedback explicit in a different way?

**3. What is the runtime's scheduling policy?**
Options 3, 4, and 6 all require the runtime to decide what executes next.
Determinism requires a fixed scheduling policy. Is non-determinism acceptable?

**4. How does this interact with the state machine model?**
The state machine is a schema that calls itself. That is Option 2 (self-referential
CHR rule) or Option 6 (fixpoint of one node). Does the state machine model
naturally extend to multi-schema coordination, or are they separate concerns?

**5. What is the programmer-visible API for declaring topology?**
Options 4 and 5 require explicit topology declarations. Options 2 and 3 are
more implicit (fire when data is ready). Which matches Evident's design
philosophy (declare what, not how)?

**6. How does the existing `..SubSchema` passthrough relate?**
Passthrough currently does flat-merge composition at parse time. How does it
fit with runtime coordination? Is it Option 1, or something different?

---

## Tentative Direction (Not a Decision)

The strongest case can be made for starting with Option 1 and extending toward
Option 2, because:

- Option 1 requires no new coordination machinery and is already almost
  implemented
- Option 2 is a natural extension of Evident's existing forward rules, not a
  new paradigm
- CHR's theoretical foundations are the closest match to Evident's existing
  constraint model
- The combination can be layered: Option 1 first, Option 2 when feedback is
  genuinely needed

Options 3, 4, 5, and 6 solve harder problems but require more design work and
introduce more programmer-visible complexity. They remain valid future directions
once the simpler base is proven.

This is a tentative observation, not a decision. The open questions above need
answers before committing.
