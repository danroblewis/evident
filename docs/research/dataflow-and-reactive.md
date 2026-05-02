# Dataflow and Reactive Architectures: A Research Document

## Overview

This document surveys dataflow programming, functional reactive programming (FRP), stream processing, and hardware dataflow architectures. The goal is to understand how these paradigms structure computation around data movement and explore implications for constraint programming languages like Evident.

---

## Part 1: Dataflow Programming Fundamentals

### 1.1 Core Model: Nodes, Edges, and Tokens

**Dataflow programs** are directed acyclic or cyclic graphs where:
- **Nodes** represent computational kernels (processes or actors)
- **Edges** are buffered communication channels carrying data
- **Tokens** are atomic data elements flowing through channels
- **Firing** occurs when a node has sufficient input data and executes its computation

The dataflow model inverts traditional von Neumann execution: instead of instructions fetched by a program counter, computation is driven by data availability. A node fires (executes) when all its input conditions are satisfied, not when some external scheduler decides.

### 1.2 Kahn Process Networks (KPN)

**Kahn process networks**, introduced by Gilles Kahn in 1974, formalize this model with deterministic semantics.

#### Communication Primitives
- **Reading**: Blocking. A process reading from an empty channel stalls until data arrives.
- **Writing**: Non-blocking. A process always succeeds in writing; channels are unbounded.
- **No testing**: Processes cannot test channels for emptiness without consuming—this prevents timing dependencies.

#### Determinism Guarantee

KPNs guarantee **timing-independent determinism**: the same input history produces identical outputs regardless of execution speed or scheduling order. This holds because:

1. Processes are monotonic: reading more tokens can only cause writing more tokens.
2. No process can test a channel's state without consuming a token.
3. Future inputs affect only future outputs.

#### Properties and Limitations

| Property | Behavior |
|----------|----------|
| **Channels** | Unbounded FIFOs in theory; bounded in practice (requires bounds analysis) |
| **Deadlock** | Can occur if cyclic dependencies aren't carefully managed |
| **Composability** | Networks can be hierarchically composed |
| **Applications** | Signal processing, embedded systems, high-performance computing, stream processing |

#### Boundedness Problem

Determining whether a KPN can run indefinitely without channels exceeding memory limits is **undecidable** for general programs. Practical solutions include:
- Design-time derivation of bounds for predictable patterns
- Dynamic FIFO growth with memory management
- Write-blocking when channels reach capacity

### 1.3 Firing Rules and Execution Models

In dataflow systems, an **actor** (computation node) defines firing rules:

```
Actor A fires when:
  - Input port 1 has N₁ tokens available
  - Input port 2 has N₂ tokens available
  - ...
  - Internal state satisfies condition X
```

Upon firing, the actor:
1. Consumes the specified tokens
2. Performs its computation
3. Produces output tokens on its output ports
4. Possibly updates internal state

**Example**: A matrix-multiply actor might fire when both input matrices are available, consume them, compute the result, and produce a single output token.

---

## Part 2: Functional Reactive Programming (FRP)

### 2.1 Core Abstractions

**Functional Reactive Programming** applies functional programming principles to reactive (event-driven, asynchronous) systems. Two primary abstractions model temporal behavior:

#### Behaviors/Signals
- **Represent continuous values** that change over time
- Can be sampled at any point
- Example: mouse position, animation state, audio amplitude
- Composed functionally: `position = integrate(velocity)`

#### Events
- **Discrete occurrences** at specific instants
- Cannot be sampled arbitrarily; only react when they fire
- Example: button click, timer tick, network message
- Composed via operators: `event1.merge(event2)`, `event.filter(pred)`

### 2.2 Key FRP Implementations

#### Elm

Elm (historically) used signals as time-varying values and events as discrete triggers. The modern Elm has moved to a different model but the historical signal/event distinction influenced FRP design.

**Key characteristic**: Decomposes asynchronous behavior into pure functions transforming signals.

#### Yampa (Haskell)

An **arrow-based** FRP for continuous-time systems.
- Uses Haskell's Arrow abstraction for composing signal transformers
- Supports continuous and discrete signals
- Efficient: works via stream transducers, not by sampling
- Applications: games, robotics, SDL/OpenGL graphics

```haskell
-- Pseudo-code structure
integral :: Signal Double -> Signal Double  -- sum of velocity -> position
wave :: Signal Double -> Signal Double      -- sin wave over time
```

#### Reflex (Haskell)

A push-pull FRP with both event and behavior abstractions:
- **Event**: Discrete occurrences with associated values
- **Behavior**: Continuously-varying values that can be sampled
- **Dynamic**: Combination of Event and Behavior for changing structure
- Works over DOM, SDL, Gloss graphics
- Efficient: pushes updates where needed, pulls only when sampled

### 2.3 Time as First-Class Concept

FRP's key innovation is **decoupling time semantics from implementation**:
- Programmers reason about continuous time / instantaneous events
- Implementation chooses sampling rate, buffering, and scheduling
- Eliminates timing bugs from manual state management

**Example**: An animation running at 60 FPS uses the same code as one at 30 FPS; only the frame rate changes.

---

## Part 3: Reactive Extensions (ReactiveX)

### 3.1 Observables and Operators

**ReactiveX** (RxJS, RxJava, RxPython, etc.) is an imperative reactive system using the **Observable** pattern:

```
Observable
  ├─ Cold: Source begins on subscription (lazy, pulls from source)
  └─ Hot: Source runs independently (eager, emits whether observed or not)
```

Operators compose observables:
- **map**: Transform each element
- **filter**: Keep matching elements
- **merge**: Combine multiple observables
- **buffer**: Collect N elements into a single emission
- **debounce**: Drop elements if too frequent
- **sample**: Take values at regular intervals

### 3.2 Backpressure and Flow Control

ReactiveX systems face a challenge: an observable emits faster than an observer can consume.

#### The Backpressure Problem
If source A emits at 1000 msgs/sec and sink B consumes at 100 msgs/sec, unbuffered systems exhaust memory maintaining queued messages.

#### Solutions in RxJS

1. **Controlled Observable**: Transform observable to respect pull requests
   - Observer explicitly requests N items
   - Observable delays emission until requested
   - Converts push → pull model

2. **Lossy backpressure**:
   - **Debounce**: Emit only after silence period
   - **Sample**: Emit only latest value at intervals
   - **Window**: Emit groups of recent items
   - **Buffer**: Collect and emit batches

3. **Hot observable strategies**:
   - pausable/pausableBuffered: Pause/resume emission
   - Proper buffering with overflow policies (drop oldest, drop newest, etc.)

**Key tradeoff**: Pull-based backpressure preserves all data but blocks producers. Lossy strategies lose data but maintain responsiveness.

---

## Part 4: Stream Processing Systems

### 4.1 Apache Flink

**Flink** is a distributed engine for stateful stream processing with event-time semantics.

#### Architecture
- **Unbounded streams** of events (vs. bounded batch processing)
- **Stateful operators** maintain keyed state across events
- **Exactly-once semantics** via distributed checkpointing
- **Event-time processing** with watermarks for handling late data

#### State Management
Flink treats state as first-class:

```
State types:
  - Keyed state: Associated with a specific key (e.g., user_id)
  - Operator state: Local to an operator instance
  - Broadcast state: Replicated across all parallel instances
```

State backends:
- **Heap memory**: Fast, limited by JVM heap
- **RocksDB**: Disk-backed, supports large state, persistent across failures

#### Windows and Aggregation
Events are grouped by time windows:
- **Tumbling**: Non-overlapping, fixed-duration (e.g., 5-minute buckets)
- **Sliding**: Overlapping by a hop interval
- **Session**: Grouped by inactivity gap
- **Custom**: Application-defined grouping

### 4.2 Kafka Streams

A lightweight library for stream processing within Kafka ecosystems.

#### Design
- **No separate infrastructure**: Runs as a library in application processes
- **Changelog topics**: State is backed by Kafka topics for durability and recovery
- **Interactive queries**: Applications can query state stores locally
- **Scaling**: Parallelism via partitions; state rebalancing on rescaling

#### Topology: Stateless vs. Stateful
- **Stateless**: map, filter, flatMap—no memory across events
- **Stateful**: count, aggregate, reduce—must retain state

### 4.3 Apache Beam

A unified programming model for batch and streaming, abstracted from execution engines (runners).

#### Key Concepts
- **PTransform**: A transformation on a collection (PCollection) of elements
- **Windowing**: Groups unbounded streams into finite windows
- **Triggers**: Determine when aggregates are emitted
- **Watermarks**: Track progress through event time

The Beam model separates *what* computation occurs from *when* and *how*:
- **What**: PTransforms (the logic)
- **When**: Windowing and triggers (the grouping)
- **How**: Runner choice (Flink, Spark, Dataflow, etc.)

---

## Part 5: Visual Dataflow Programming

### 5.1 Max/MSP and Pure Data

**Max** (commercial) and **Pure Data** (open source) are graphical environments for music, audio, and control:

#### Programming Model
- Objects are represented as boxes
- Connections (wires) represent data flow
- Data flows from outlet to inlet as messages or audio signals
- Objects can have both inlet/outlet sets and explicit connections
- Execution is message-driven: inlets receive events, triggers computation

#### Characteristics
- Real-time audio processing
- Visual feedback on data flow
- Extensible via external objects
- Scheduling: messages and audio processed in separate scheduler thread

### 5.2 LabVIEW

National Instruments' visual language uses **graphical dataflow**:

#### Dataflow Execution
- Nodes (functions) are wired together with data flowing through edges
- A node executes when **all inputs are available**
- Output data immediately flows to dependent nodes
- No explicit sequencing; order emerges from data dependencies

#### Advantages
- Intuitive visual representation
- Implicit parallelism: independent branches execute simultaneously
- Type safety via wire colors and types

#### Applications
- Test and measurement automation
- Instrument control
- Real-time embedded systems
- Signal processing

### 5.3 Scratch

MIT's visual language for education:

- Blocks represent operations (sensing, motion, sounds, logic)
- Connections form sequences or loops
- Event-driven execution (on key press, on click, etc.)
- Simplified dataflow for learning

---

## Part 6: Synchronous Dataflow (SDF)

### 6.1 Core Restriction

**Synchronous Dataflow** restricts Kahn process networks by fixing token rates:

**Definition**: Each actor has fixed consumption/production rates. If actor A consumes M tokens from port X and produces N tokens to port Y on each firing, these numbers are constants (known at compile time).

### 6.2 Static Scheduling

With fixed rates, the system can compute a **schedule** at compile time:

```
Schedule for a network:
  Actor A fires 3 times
  → Actor B fires 5 times
  → Actor C fires 2 times
  (numbers depend on token rate ratios)
```

Benefits:
- No runtime scheduling overhead
- Bounded FIFO channels (in some cases provable)
- Predictable resource usage
- Can target parallel hardware directly

### 6.3 Determinism and Restrictions

SDF graphs are deterministic **but restrictive**:
- No data-dependent conditionals (if-then-else on data values)
- No data-dependent loops
- No dynamic token rate changes
- All decisions must be static (compile-time)

Example that breaks SDF:
```
while data > 0:      // ❌ Loop count depends on data value
  process(data)
```

Equivalent SDF:
```
// ❌ Not possible; must unroll to fixed iterations
```

### 6.4 Applications

SDF excels for **regular, predictable computations**:
- Digital signal processing (FIR filters, FFTs)
- Video processing (frame-based operations)
- Embedded systems with static workloads
- Hardware synthesis for FPGAs

---

## Part 7: The Actor Model and Distributed Dataflow

### 7.1 Core Principles

The **actor model**, introduced by Carl Hewitt in the 1970s, treats actors as concurrent computation units that:

1. Receive messages from other actors
2. Process messages (execute behavior)
3. Send messages to other actors
4. Create new actors
5. Determine handling strategy for the next message

### 7.2 Communication

**Asynchronous message passing**:
- Decouples sender from receiver
- Messages are immutable
- No ordering guarantee between messages (implementation may reorder)
- Enables optimization like packet switching
- Locality principle: actors only communicate with addresses they know

### 7.3 Actors vs. Dataflow

**Actor systems** and **dataflow networks** differ:

| Aspect | Actors | Dataflow |
|--------|--------|----------|
| **Network structure** | Dynamic (create/destroy at runtime) | Static (fixed at definition) |
| **Communication** | Point-to-point messages | Buffered channels |
| **State** | Mutable local state | Tokens/data in flight |
| **Abstraction** | Concurrent processes | Token flow and rates |

**However**: Dataflow can be viewed as a restricted actor model with fixed network topology and explicit data-driven triggering.

### 7.4 Resilience and Supervision

Actor frameworks (Akka, Pekko) add supervision hierarchies:
- Parent actors monitor children
- On failure, parent can restart child
- Cascading restarts form a tree of responsibility
- Enables fault-tolerant distributed systems

---

## Part 8: Hardware Dataflow

### 8.1 Systolic Arrays

**Systolic arrays** implement dataflow at the circuit level—a mesh of tightly coupled processing elements (PEs) executing in lockstep.

#### Architecture
```
Input ──┬─→ PE ──┬─→ PE ──┬─→ PE ──→ Output
        │        │        │
    Internal  Internal  Internal
    Memory   Memory    Memory
```

Each PE:
- Reads data from neighbors (or external input)
- Performs a fixed computation
- Writes results to neighbors (or external output)
- Stores partial results or weights

#### Execution Model

**Transport-triggered**: Data availability at inputs triggers computation, not instructions.

Multiple data flows move through the array simultaneously:
- **Data flow X**: Moves horizontally (e.g., matrix A rows)
- **Data flow Y**: Moves vertically (e.g., matrix B columns)
- **Partial results**: Accumulate through the PE mesh

Classic example: matrix multiplication C = A × B
```
A rows flow left-to-right
B columns flow top-to-bottom
C partial sums accumulate in-place
Output flows as complete C rows
```

#### Dataflow Patterns

**Weight-Stationary (WS)**
- Weights stay in place (high reuse)
- Activations stream through
- Partial sums stream through
- Reduces off-chip bandwidth for weights

**Output-Stationary (OS)**
- Partial results stay in PEs
- Activations and weights stream through
- Simplifies accumulation logic

**Activation-Stationary (AS)**
- Activations stay in PEs
- Weights and partial results stream through
- Minimizes activation bandwidth

#### Throughput Advantages

1. **No external memory access**: All data lives on-chip during computation
2. **Pipelining**: Multiple data items progress through array simultaneously
3. **Parallelism**: All PEs work in parallel
4. **Locality**: Nearest-neighbor communication minimizes latency
5. **Deterministic timing**: Lockstep execution, predictable latency

### 8.2 FPGAs and Dataflow Synthesis

**Field-Programmable Gate Arrays** can be synthesized to implement arbitrary dataflow topologies:

#### Hardware Components
- **DSP Slices**: Hard-wired multiply-add units (MACs); configurable for int8, int4, low-precision formats
- **Block RAM (BRAM)**: Dual-port memories; serve as weight buffers, line buffers, accumulators
- **Lookup Tables (LUTs)**: Implement arbitrary logic
- **Interconnect**: Programmable routing between components

#### High-Level Synthesis (HLS)
Tools like Vivado HLS or AutoSA compile dataflow descriptions to FPGA hardware:
- Input: Dataflow graph or loop-unrolled C code
- Output: FPGA configuration that implements the dataflow
- Benefits: No manual HDL (Verilog/VHDL) writing
- Trade-offs: Less control than hand-optimized HDL

#### Systolic Array Synthesis
Tools generate parameterized systolic arrays:
- Specify array dimensions, data types, computations
- Tool generates PE templates and interconnect
- Supports different dataflow patterns (WS, OS, AS)
- Maps to FPGA resources automatically

---

## Part 9: Implications for Evident

### 9.1 Constraint Schemas as Dataflow Nodes

Evident programs are collections of constraints forming a schema. Each schema can be viewed as a **dataflow node** in a larger computation:

```
[Schema: Task]
  inputs: duration, cost
  outputs: satisfying binding over {duration, cost}
  firing rule: when constraints are defined and Z3 can solve

[Schema: Resource]
  inputs: availability
  outputs: binding
  ...

[Network]: Compose Task and Resource schemas
           data flows through variable bindings
```

### 9.2 Variable Bindings as Tokens

In Evident:
- **Tokens** = variable bindings (assignments to concrete values or model elements)
- **Token rate** = how many variables are bound per query
- **Firing** = running Z3 solver when all constraints are satisfied

### 9.3 Buffering and Deadlock

Implications from dataflow theory:

#### Unbounded Buffers
Evident's current architecture doesn't persist tokens across queries. Each `/query` invocation solves independently. If we were to compose schemas hierarchically with buffering:

- Channels would buffer partial solutions
- Deadlock is possible if schemas have circular dependencies
- Bounds analysis would become necessary for production systems

#### Scheduling
Currently, execution order (which schema to query first) is programmer-directed. Dataflow theory suggests:

1. **SDF model**: If token rates are predictable, precompute execution order
2. **KPN model**: If rates vary, use dynamic scheduling with monotonicity guarantees
3. **Actor model**: If schemas are independent, parallelize queries; buffering maintains eventual service

### 9.4 Flow Control and Backpressure

Dataflow raises a question: **what happens if a schema produces solutions faster than consumers need?**

In current Evident:
- Queries are on-demand
- No buffering of solutions
- No backpressure mechanism

If Evident were to support streaming or batched queries:
- Could adopt pull-based backpressure (consumer requests N solutions)
- Could use lossy strategies (sample N random solutions)
- Could implement bounded state (SDF-style fixed rates)

### 9.5 Composability and Hierarchy

Dataflow systems support hierarchical composition:
- Subgraphs (sub-schemas) become reusable nodes
- Connections between nodes (variable bindings) are explicit
- Type checking (sort matching) ensures safe composition

Evident's current design supports this via schema composition (task.duration accesses sub-fields). Dataflow formalism would clarify:
- Which compositions are safe (no cycles)?
- What are the data rates of composed systems?
- Can we statically schedule composite schemas?

### 9.6 Determinism and Reproducibility

Kahn process networks guarantee timing-independent determinism. For Evident:

- **Determinism goal**: Same query always returns the same solution
- **Current state**: Solver is deterministic (Z3 is deterministic), but solution selection (first vs. all) is programmer-directed
- **Dataflow perspective**: Determinism is automatic if execution is data-driven (not timing-dependent)

This aligns naturally with constraint solving: satisfaction is data-dependent, not scheduling-dependent.

### 9.7 State Management

Dataflow distinguishes:
- **Stateless** actors: Pure functions mapping inputs to outputs
- **Stateful** actors: Maintain internal memory across firings

Evident's constraints are mostly stateless (pure logical constraints). But:
- Z3 solver state (assertions, scopes) is stateful
- Multiquery scenarios might maintain state across queries
- Schema composition carries state (which variables are bound)

Dataflow theory suggests explicit state management with clear semantics (e.g., RocksDB-style persistent state for reliability).

---

## Part 10: Synthesis and Design Insights

### 10.1 Scheduling Strategies for Evident

**From SDF**: If token rates are predictable
- Pre-compute firing order at compile-time
- Verify bounded FIFO channels
- Generate fixed schedules for embedded deployment

**From KPN**: If rates vary
- Execute schemas when all inputs are available
- Use dynamic scheduling with work-stealing or priority queues
- Guarantee monotonicity (more inputs → more outputs)

**From Actors**: If schemas are independent
- Query multiple schemas in parallel
- Use message passing for variable binding communication
- Implement supervision for fault tolerance

### 10.2 Buffering and Deadlock Prevention

**Deadlock risk**: Circular schema dependencies with finite buffering

```
Schema A reads from B
Schema B reads from A
```

**Prevention strategies**:
1. **Static analysis**: Detect cycles at definition time; flag as errors
2. **SDF approach**: Bound buffers and prove liveness via rate analysis
3. **KPN approach**: Use unbounded FIFOs; requires garbage collection
4. **Actor approach**: Timeouts and supervision hierarchies

### 10.3 Backpressure for Streaming Queries

If Evident supports streaming (continuous input of constraints):

**Push model** (current):
- Query returns one solution
- Backpressure falls on caller (must decide when to query again)

**Pull model** (dataflow-inspired):
- Caller requests N solutions
- Evident's solver controls emission rate
- Natural for batch processing

**Hybrid**:
- Allow both on-demand queries (push) and batch pulls (pull)
- Implement lossy strategies (sample, reservoir sampling) for infinite solution spaces

### 10.4 Type System and Sort Matching

Dataflow systems use types to ensure safe composition:

```
SourceActor(outputs: Int64)
  ↓
Transform(inputs: Int64, outputs: Float)
  ↓
SinkActor(inputs: Float)
```

Evident already has this via sorts:
- SortRegistry ensures type safety
- Composed schemas must match field types
- Type errors are caught at definition, not runtime

### 10.5 Visualization and Developer Experience

Dataflow systems (Max/MSP, Pure Data, LabVIEW, Scratch) succeed because wiring diagrams are intuitive. For Evident:

**Current**: Text-based constraint language
**Opportunity**: Visual composition view
- Schemas as boxes (with schema name, inputs/outputs)
- Edges represent variable bindings
- Drag-and-drop composition
- Type-safe wiring (sorts on ports prevent mismatches)

This aligns with LabVIEW's success: visual representation of logic flow.

---

## Part 11: Related Work and Further Reading

### Dataflow and Constraint Solving
- **Ptolemy II** (UC Berkeley): A framework for modeling and design of concurrent systems, including dataflow and constraint domains
- **StreamIt**: A language for stream processing with explicit dataflow syntax
- **Lustre/Esterel**: Synchronous programming languages (SDF-like) for reactive systems

### Reactive and Constraint Systems
- **Constraint Handling Rules (CHR)**: A language for writing constraint solvers; naturally expressive of dataflow-like rules
- **Scenic**: A language for specifying constraint scenarios in autonomous systems
- **DrRacket and #lang Dataflow**: Educational dataflow language in Racket

### Hardware-Software Codesign
- **AutoSA**: Automatic systolic array synthesis from loop descriptions
- **Halide**: A language for image processing; compiles loops to dataflow schedules
- **Glow**: Compiler for neural networks targeting dataflow hardware

---

## Conclusion

Dataflow programming provides a rich set of abstractions for reasoning about computation through data movement:

1. **Kahn Process Networks** establish timing-independent determinism via blocking reads and non-blocking writes.

2. **Synchronous Dataflow** enables static scheduling by fixing token rates, crucial for embedded and hardware systems.

3. **Functional Reactive Programming** brings composability and time-abstraction to asynchronous systems.

4. **Stream processing systems** (Flink, Kafka, Beam) operationalize dataflow for scalable, stateful computation over unbounded data.

5. **Visual dataflow** languages demonstrate that wiring diagrams are intuitive and cognitively effective.

6. **Hardware dataflow** (systolic arrays, FPGAs) show dataflow's natural fit for circuit design and parallel computation.

7. **The actor model** unifies message-passing concurrency with dataflow semantics, enabling distributed systems.

For **Evident**, these insights suggest:
- Schemas are nodes; variable bindings are tokens
- Execution is data-driven; determinism follows automatically
- Composition requires explicit type safety (sorts)
- Buffering and scheduling have proven solutions (SDF, KPN, actor strategies)
- Visualization could significantly improve developer experience
- Streaming and batch queries map naturally to dataflow pull/push models

Dataflow theory doesn't directly change Evident's solver (Z3 remains the engine), but it clarifies semantics for schema composition, execution ordering, buffering, and multi-query orchestration—critical for production deployment.

