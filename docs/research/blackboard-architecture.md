# The Blackboard Architecture: A Design Pattern for Coordinated Constraint Solving

## Executive Summary

The **Blackboard Architecture** is a foundational AI design pattern for systems where multiple independent specialist modules must solve complex, ill-structured problems through coordinated opportunistic reasoning. Originated in the 1970s with HEARSAY-II (speech recognition), the pattern has proven effective for domains ranging from medical diagnosis to robotics to distributed constraint solving.

This document explores blackboard theory and its direct application to Evident: a constraint programming language where schemas are knowledge sources, the solver's variable bindings form the shared blackboard state, and the runtime acts as the controller coordinating which constraints to solve next.

---

## Part 1: The Blackboard Architecture — Core Concepts

### 1.1 What Is the Blackboard Pattern?

The blackboard architecture provides a computational framework for systems that integrate diverse specialized modules and implement complex, non-deterministic control strategies. Rather than a rigid top-down execution plan, the system is data-driven: execution unfolds based on the current state of a shared, persistent working memory called the **blackboard**.

**Three essential components:**

1. **The Blackboard** — A shared, structured repository containing:
   - The problem specification and input data
   - Partial solutions and intermediate hypotheses
   - All contributions from knowledge sources
   - Organized to support efficient pattern matching (e.g., hierarchies of abstraction levels, time intervals, confidence scores)

2. **Knowledge Sources (Specialists)** — Independent, domain-specific modules that:
   - Encapsulate domain expertise or algorithmic capability
   - Are *not* directly coupled to each other
   - Inspect the blackboard state and decide if they can contribute
   - Update the blackboard with partial results when activated
   - Have well-defined triggering conditions ("when conditions X are met on the blackboard, I can act")

3. **Control Component (Controller/Scheduler)** — The orchestrator that:
   - Continuously monitors the blackboard state
   - Decides *which* knowledge source should act next
   - Manages the execution order and frequency
   - Does not know domain-specific details—only domain-independent heuristics for scheduling
   - May itself use the blackboard to reason about control decisions (meta-level reasoning)

**Key principle:** Knowledge sources communicate *only through the blackboard*. There is no direct coupling, no message passing between specialists, no shared internal state. This decoupling is essential for modularity and reusability.

### 1.2 The Execution Model: Opportunistic Problem Solving

Traditional systems follow explicit, predetermined control flow. The blackboard pattern embraces **opportunistic problem solving**: the system's next action is determined by opportunity, not plan.

At each step:

1. **Evaluate readiness**: The controller scans registered knowledge sources to determine which have triggering conditions satisfied by the current blackboard state.
2. **Rate candidates**: For each ready knowledge source, compute a priority or rating score (e.g., estimated relevance, urgency, cost-benefit).
3. **Select and execute**: Run the highest-rated knowledge source. It reads the blackboard, performs computation, and may post new hypotheses or refine existing ones.
4. **Update state**: The blackboard is updated with new results.
5. **Repeat**: The controller re-evaluates readiness given the new state. New opportunities may emerge; others may be exhausted.

The process terminates when either:
- A solution is found (goal state reached on the blackboard)
- No knowledge sources are ready (no progress possible—may indicate no solution exists)
- Resource limits are exceeded (time, iterations, memory)

**Opportunism vs. Plan**: A planned system decides in advance: "First do A, then B, then C." An opportunistic system decides at runtime: "What can we usefully do right now given the current state?" This flexibility is powerful for problems where the solution path is unpredictable (e.g., speech understanding—low-level acoustic features can trigger high-level linguistic hypotheses unexpectedly).

### 1.3 Focus of Attention

Real-world blackboards can grow large, and evaluating all knowledge sources at every step is expensive. The **focus of attention** mechanism restricts the controller's search:

- **Agenda**: A priority queue of "interesting" areas on the blackboard (e.g., hypotheses needing refinement, uncertain zones, recently changed regions).
- **Refocusing**: Instead of scanning all KSs against the entire blackboard, the controller narrows its view to the high-priority region.
- **Dynamic shifting**: As problem-solving progresses, focus can shift (e.g., from low-level acoustic features to phonemes to words to sentences).

In HEARSAY-II, the focus-of-attention mechanism tracked "levels of representation" (acoustic, phonetic, phonemic, lexical, syntactic, semantic) and "time intervals" (where in the speech signal uncertainty remained), allowing the system to focus computational effort where it was most needed.

**Benefit**: This converts a potentially O(n) evaluation (all KSs × all blackboard regions) into O(1) by precomputing which KSs are relevant to each region.

---

## Part 2: Historical Context — From HEARSAY-II to Modern Descendants

### 2.1 HEARSAY-II (1971–1976)

**The Problem**: Continuous speech recognition from a 1000-word vocabulary with high acoustic noise and ambiguity.

**The Insight**: No single algorithm (acoustic models, language models, parsing) is strong enough alone. Different knowledge sources can contribute complementary perspectives:
- **Acoustic specialist**: Analyzes raw signal → phonetic hypotheses
- **Phonetic specialist**: Phonemes → higher-confidence phoneme sequences
- **Lexical specialist**: Phoneme sequences → candidate words
- **Syntactic specialist**: Word sequences → grammatically plausible parses
- **Semantic specialist**: Semantic plausibility filters

These specialists operate at different levels of abstraction and are triggered by different conditions. The breakthrough was recognizing that these need not run in a strict pipeline—instead, hypotheses at any level can trigger specialists at other levels. A lexical hypothesis (word candidate) can refine acoustic hypotheses; a semantic rejection can backtrack to acoustic analysis.

**Architecture**:
- **Blackboard**: A 3D structure (representation level × time interval × alternative hypotheses). E.g., at time 100ms, phoneme level, three competing hypotheses each with a confidence score.
- **Knowledge Sources**: ~100 small programs (50–200 lines each), each a specialist. Triggering conditions: "if new phoneme hypotheses exist in interval [t1, t2]" or "if word hypotheses don't form a grammatical parse."
- **Controller**: Event-driven. When a KS updates the blackboard, affected KSs are activated. Uses a priority queue (agenda) and **focus of attention** to limit the search space.

**Results**: Achieved 90% accuracy on the task, a remarkable result for the 1970s.

**Why it mattered**: HEARSAY-II demonstrated that opportunistic, data-driven cooperation could solve problems that rigid pipelines couldn't. It became the blueprint for blackboard systems.

### 2.2 HASP/SIAP (1973–1975)

**Domain**: Passive sonar signal interpretation—analyzing underwater acoustic signals to classify ships, submarines, and activities.

**Contribution**: HASP extended HEARSAY-II's ideas to a hierarchical, event-based control architecture:
- Lower-level KSs detect acoustic events (frequency bins, transients).
- Middle-level KSs synthesize events into platform-level hypotheses (e.g., "diesel engine at bearing 045").
- Top-level KSs maintain tactical situation awareness.

**Innovation**: Event-based triggering. When a KS posts a result, interested KSs are automatically notified (like publish-subscribe). This was more efficient than the controller polling all KSs at each cycle.

### 2.3 BB1 — The Meta-Level Blackboard (1983)

**Problem**: Earlier blackboards required hardcoded control heuristics. BB1 asked: *Can we apply the blackboard model to control itself?*

**Solution**: 
- **Domain blackboard**: Records partial solutions (as in HEARSAY-II).
- **Control blackboard**: Records control hypotheses (e.g., "pursue strategy A," "switch to strategy B," "backtrack to state X").
- **Meta-level KSs**: Monitor domain progress and propose control decisions. If domain-level KSs are stalled, meta-level KSs can recognize this and suggest a different strategy.

**Example**: In constraint satisfaction, if all KSs are idle (stalled on a difficult subproblem), a meta-level KS might suggest:
- Try a different variable ordering
- Relax constraints temporarily to find a partial solution
- Switch to a stochastic algorithm

BB1 applied opportunistic scheduling to its own control, making blackboard systems **self-aware** and **adaptive**.

**Applications**:
- Assembly and manufacturing planning
- 3D protein structure inference (X-ray crystallography)
- Intelligent tutoring systems (meta-level reasoning about student misconceptions)
- Real-time patient monitoring

### 2.4 Modern Descendants

**GBB (Generic Blackboard)** (1990s–2000s): Improved efficiency through:
- Multi-dimensional blackboards (not just 3D)
- Faster pattern matching (indexing and partial evaluation)
- Constraint propagation integration

**Bayesian Blackboards**: Extend the blackboard to probabilistic reasoning. KSs post hypotheses with confidence/probability; the control component uses Bayesian inference to decide priority.

**Event-Driven Blackboards**: Combine blackboard principles with pub-sub messaging for distributed systems (e.g., sensor networks, IoT). KSs are microservices publishing to an event bus; the blackboard is a distributed state store.

**Game AI**: Blackboards are now standard in game development for AI behavior trees and decision-making. Multiple goal-seeking agents update a shared state, and priorities shift dynamically.

**Military C4ISTAR**: Command, Control, Communications, Computers, Intelligence, Surveillance, Target Acquisition, and Reconnaissance systems use blackboards for multi-sensor fusion and situation assessment.

---

## Part 3: The Control Problem — How Does the System Decide What to Do Next?

This section focuses on the core challenge: **scheduling**. How does the controller decide which KS should execute next?

### 3.1 The Agenda and the Scheduler

The **agenda** is a priority queue of ready knowledge sources. When a KS updates the blackboard, the system identifies which other KSs might now have satisfied triggering conditions and adds them to the agenda.

The **scheduler** removes the highest-priority KS from the agenda and executes it.

Priority can be computed using:

**1. Attentional Priority**: Explicit priority numbers assigned to KSs (e.g., phoneme specialist = 50, semantic specialist = 30). Lower numbers = higher priority.

**2. Urgency**: How recently was the relevant blackboard region updated? Older hypotheses may need refinement; newer ones are still "hot."

**3. Relevance Heuristics**: Domain-specific scoring. Example (speech understanding):
```
priority = 0.3 × phonetic_confidence 
         + 0.2 × num_supporting_hypotheses 
         + 0.5 × coverage (how many phonemes have hypotheses?)
```

**4. Cost-Benefit**: Estimated cost of running a KS vs. benefit of its output.
```
priority = expected_benefit / computational_cost
```

**5. Goal Proximity**: Distance to the solution state.
```
priority = (goal_value - current_value) / (estimated_steps_needed)
```

### 3.2 Rescheduling After Each Step

Crucially, **priorities are re-evaluated after every KS execution**. The blackboard has changed, so triggering conditions change, and cost-benefit estimates shift.

Example (constraint satisfaction):
1. Controller executes KS_A, which constrains variable x to domain {1, 3, 5}.
2. Re-evaluation: KS_B's triggering condition "if x has a domain of size ≤ 3" is now true.
3. KS_B is added to the agenda.
4. Meanwhile, KS_C's condition "if domain of y is size > 100" is still false; it stays off the agenda.
5. Scheduler picks the next highest-priority KS (maybe KS_B) and repeats.

This dynamic rescheduling is crucial for opportunism. Without it, the system would stick to a plan even as better opportunities emerge.

### 3.3 BB1's Control Blackboard Approach

BB1 introduced a more sophisticated model: control decisions are themselves hypotheses on the blackboard.

**Meta-level KSs** propose control strategies:
- "I recommend pursuing depth-first search next."
- "Current strategy is stalled; try randomized restart."
- "Backtrack to choice point X and try a different branch."

Control hypotheses are rated by meta-control knowledge sources:
- "How well did the last strategy work?"
- "Are we making progress?"
- "How much computational budget remains?"

This allows the system to *learn* what control strategies work best over time and adapt dynamically.

### 3.4 Distributed Scheduling: Event-Based Activation

In distributed blackboard systems (e.g., sensor networks), we can't poll all KSs centrally. Instead:

1. Each KS monitors the blackboard for changes.
2. When its triggering condition becomes satisfied, it self-activates (publishes a message, adds itself to a queue, etc.).
3. A **distributed scheduler** (e.g., a message broker) routes work to available processors.

This is essentially a hybrid of the traditional blackboard model and the **actor model** / **pub-sub pattern**.

---

## Part 4: Knowledge Sources and the Decoupling Principle

### 4.1 What Makes a Good Knowledge Source?

A knowledge source is:
- **Focused**: Solves a specific sub-problem or applies a specific algorithm.
- **Encapsulated**: All inputs come from the blackboard; all outputs go to the blackboard.
- **Independent**: Does not call other KSs; does not maintain shared mutable state with other KSs.
- **Reusable**: Can be applied in different contexts / different blackboards.
- **Stateless**: Or state is stored on the blackboard, not locally within the KS.

### 4.2 Triggering Conditions (Preconditions)

Each KS registers a **triggering condition**, a predicate over the blackboard state.

Examples:
```
KS_phoneme_to_lexical:
  Trigger: "If phoneme hypotheses exist for interval [t1, t2]
            and no lexical hypotheses yet exist for that interval"
  Action: Try to match phoneme sequences to words in dictionary

KS_constraint_propagation:
  Trigger: "If any variable's domain has been reduced
            since the last arc-consistency pass"
  Action: Run arc-consistency to prune other variables' domains

KS_backtrack:
  Trigger: "If all ready KSs are off the agenda (deadlock)
            and a partial solution exists"
  Action: Undo the last choice and try an alternative
```

**Advantage**: KSs declare their preconditions explicitly, enabling the controller to efficiently determine which KSs are runnable without invoking them.

### 4.3 No Direct Coupling

The blackboard pattern *forbids* direct calls between KSs. Compare:

**Forbidden (tight coupling)**:
```python
class PhonemeKS:
    def run(self):
        ...
        lexical_ks = LexicalKS()
        lexical_ks.process(self.phonemes)  # Direct call!
```

**Correct (blackboard coupling)**:
```python
class PhonemeKS:
    def run(self, blackboard):
        ...
        blackboard.add_hypotheses("phonemes", self.results)
        # LexicalKS checks blackboard independently
        # PhonemeKS doesn't know or care about LexicalKS

class LexicalKS:
    def triggered(self, blackboard):
        return blackboard.has_hypotheses("phonemes")
    
    def run(self, blackboard):
        phonemes = blackboard.get_hypotheses("phonemes")
        ...
        blackboard.add_hypotheses("lexical", self.results)
```

This decoupling has profound implications:
- KSs can be added, removed, or replaced without recompiling others.
- KSs can run in parallel (if the blackboard is thread-safe).
- Testing KSs in isolation is straightforward (mock the blackboard).

---

## Part 5: Comparison to Other Architectures

### 5.1 Blackboard vs. Actor Model

| Aspect | Blackboard | Actor Model |
|--------|-----------|------------|
| **Communication** | Shared mutable state (blackboard) | Message passing |
| **Coupling** | KSs decoupled via blackboard | Actors directly reference each other |
| **Control** | Central scheduler (controller) | Distributed; each actor decides when to process messages |
| **Visibility** | All KS outputs visible on blackboard | Only recipient of message sees it |
| **Backtracking** | Possible; undo is part of KS logic | Harder; requires explicit rollback protocols |
| **Best for** | Ill-structured problems requiring broad search | Responsive, distributed systems with message queues |

**Example**: In an actor-based speech recognizer, PhonemeActor sends a message directly to LexicalActor. If LexicalActor is busy or the message ordering is wrong, coordination breaks. In a blackboard, PhonemeKS posts to the blackboard; LexicalKS independently checks the blackboard and acts when ready. The blackboard decouples timing.

### 5.2 Blackboard vs. Microservices

| Aspect | Blackboard | Microservices |
|--------|-----------|--------------|
| **Shared State** | Yes (blackboard) | No; each service manages its own state |
| **Service Discovery** | Implicit (all KSs know the blackboard interface) | Explicit (service registry, DNS, etc.) |
| **Eventual Consistency** | Not required; blackboard can be strongly consistent | Expected; updates propagate asynchronously |
| **API** | Blackboard read/write | HTTP, gRPC, message queues |
| **Deployment** | Often monolithic (all KSs in one process) | Distributed across machines/containers |
| **Latency** | Low (shared memory) | Higher (network round-trips) |

**Modern hybrid**: Distributed event-driven blackboards (e.g., using Kafka or Redis) combine blackboard and microservice ideas. Each microservice publishes events to a shared event stream (the distributed blackboard); others subscribe and react.

### 5.3 Blackboard vs. Pub-Sub / Event-Driven Systems

| Aspect | Blackboard | Pub-Sub |
|--------|-----------|---------|
| **Message Persistence** | State persists on blackboard | Messages often ephemeral |
| **State Queries** | KSs can query the full history/state | Subscribers see only new messages |
| **Backtracking** | Natural; undo previous updates | Harder; no central undo mechanism |
| **Control** | Central scheduler rates priorities | Subscribers react asynchronously |
| **Complex Queries** | Efficient (e.g., "find all unsolved constraints") | May require re-aggregating all messages |

**Key difference**: A pub-sub message is ephemeral ("here's an event, react to it"). A blackboard entry persists ("this is the current state; anyone interested can read it"). For problems requiring a consistent, queryable state (like constraint solving), blackboard is more natural.

---

## Part 6: Applying Blackboard Theory to Evident

Now we connect blackboard architecture to Evident's design. Evident is a constraint programming language where:
- **Schemas** are collections of variables and constraints.
- **Queries** ask whether a satisfying assignment exists.
- The **Z3 solver** finds assignments.

How does blackboard theory apply?

### 6.1 Mapping Evident to Blackboard Components

**Blackboard**: The solver's current state — variable assignments, domains, constraints.
- Each variable has a domain (initially infinite or specified by a type).
- As the solver progresses, domains are narrowed (constraint propagation).
- Partial assignments are recorded.

**Knowledge Sources (Specialists)**: Multiple solving strategies or constraint types.
- **Arithmetic constraint specialist**: Handles `x + y = 10`. Triggers when both x and y have narrowed domains.
- **Logical constraint specialist**: Handles `x ∧ ¬y`. Triggers when propositional variables are bound.
- **Set membership specialist**: Handles `item ∈ Set`. Triggers when Set's membership conditions change.
- **Search strategy specialist**: When deterministic constraint propagation exhausts possibilities, this KS makes a choice (e.g., pick an unbound variable and try a value).
- **Backtracking specialist**: When a choice fails, this KS undoes it and tries an alternative.

**Controller**: Evident's runtime / solver.
- Monitors which constraints are ready to fire.
- Decides which to evaluate next.
- Manages the search (forward, backtrack, etc.).

### 6.2 Opportunistic Constraint Solving

Current constraint solvers (e.g., Z3) use sophisticated heuristics but often internal to the solver—not transparent to users. A blackboard view suggests:

1. **Make strategies explicit** as knowledge sources. Instead of burying heuristics in Z3, expose:
   - Arc consistency propagation
   - Constraint-specific reasoning (e.g., all-different)
   - Variable and value ordering heuristics
   - Local search moves (for optimization)

2. **Opportunistic scheduling**: Which constraint or propagation rule should fire next?
   - A constraint that was just narrowed may have learned a lot; prioritize KSs that depend on it.
   - A variable that is bottleneck for many constraints should be solved early.
   - A constraint that is "almost satisfied" may be worth focusing on.

3. **Focus of attention**: Not all constraints are equally important at each moment.
   - Early in solving: focus on high-impact constraints (those that affect many variables).
   - When most variables are solved: focus on residual constraints.
   - In optimization: focus on constraints related to the objective.

**Example in Evident**:
```
schema Task {
  id: Int,
  duration: Int,
  depends_on: Task?,
  deadline: Int,
}

query: task1 ∈ Task, task2 ∈ Task,
       task1.deadline = 100,
       task2.deadline = 150,
       task1.depends_on = task2,
       ...
```

Solving this:
1. **Constraint propagation specialist** fires: `task1.depends_on = task2` → both variables must exist and be compatible.
2. **Schema field specialist** fires: Expanding `task1.duration` and `task2.duration` as sub-variables.
3. **Ordering specialist** fires: Recognizing `task1.depends_on = task2` creates a precedence constraint; prioritizing these variables early.
4. **Search specialist** fires: Assigning concrete values to `id`, `duration`, etc.

Each specialist acts only when its preconditions (trigger) are met, and the controller decides the order.

### 6.3 Multi-Schema Coordination via Blackboard

One of Evident's key features is **schema composition**:
```
schema Task { ... }
schema Project { tasks: Set<Task>, ... }

query: project ∈ Project, ...
```

A blackboard view clarifies coordination:
- **Schema expansion specialist** (KS): When `project ∈ Project` is posted, expand it into `project.tasks`, `project.id`, etc., and post these as new variables.
- **Set constraint specialist** (KS): When `project.tasks` has membership constraints, refine them.
- **Per-task specialist** (KS): When a specific task in the set is being solved, apply task-specific rules.

Without an explicit blackboard, these steps might be hardcoded in the runtime. With a blackboard view, they are modular KSs, reorderable, and replaceable.

### 6.4 Control Strategy as Meta-Level Reasoning

Following BB1, Evident could support:
- **Control blackboard**: Record meta-level hypotheses ("try breadth-first search," "switch to local search," "backtrack and try a different branch").
- **Meta-level KSs**: Monitor solver progress and suggest control changes.
  - Detect stalling: "No progress for 1000 iterations" → switch strategies.
  - Detect oscillation: "Backtracking to the same choice point repeatedly" → relax constraints or restart.
  - Monitor resource usage: "Time budget nearly exhausted" → heuristic approximation.

This would make Evident's solver **self-aware** and **adaptive**.

### 6.5 Extensibility via KS Registration

A blackboard architecture makes Evident extensible. Users could register custom KSs:

```python
# In Evident user code
class MyConstraintSpecialist:
    def triggered(self, blackboard):
        return blackboard.has_variable("my_custom_type")
    
    def run(self, blackboard):
        var = blackboard.get_variable("my_custom_type")
        # Apply custom reasoning
        blackboard.update_domain(var, new_domain)

evident_runtime.register_knowledge_source(MyConstraintSpecialist())
```

This is cleaner than modifying the runtime directly and aligns with blackboard extensibility principles.

### 6.6 Distributed and Streaming Versions

A blackboard view also opens doors to:

1. **Distributed solving**: Multiple solvers on different machines contribute constraint reasonings to a shared blackboard (e.g., Redis or Kafka).

2. **Streaming constraints**: As new constraints arrive (e.g., from a live data source), KSs react opportunistically without re-solving from scratch.

3. **Interactive problem-solving**: A user adds a constraint, the controller re-prioritizes KSs, and the solver incrementally refines the solution.

---

## Part 7: Design Principles for Evident's Blackboard Runtime

Based on blackboard theory, here are recommendations for Evident's control architecture:

### 7.1 Make Constraint Classes Into Knowledge Sources

Instead of:
```python
# Current (monolithic)
def solve(constraints):
    while not solved():
        apply_arc_consistency()
        apply_alldiff_constraint()
        if stuck:
            search()
```

Move to:
```python
# Blackboard-based
class ArcConsistencyKS(KnowledgeSource):
    def triggered(self, bb):
        return bb.has_narrowed_domain_since_last_run("ac")
    
    def run(self, bb):
        for var in bb.variables:
            bb.narrow_domain(var, arc_consistent_values(var))

class AllDiffKS(KnowledgeSource):
    def triggered(self, bb):
        return bb.has_constraint_of_type("all_different")
    
    def run(self, bb):
        for constraint in bb.constraints_of_type("all_different"):
            # Apply all-different propagation

class SearchKS(KnowledgeSource):
    def triggered(self, bb):
        return not bb.is_solved() and bb.no_progress_since(threshold)
    
    def run(self, bb):
        var, value = select_variable_value(bb)
        bb.set_variable(var, value)
        bb.push()  # Backtrack point
```

### 7.2 Implement a Scheduler with Priority Heuristics

```python
class Scheduler:
    def rate_knowledge_source(self, ks, blackboard):
        """Compute priority for a ready KS."""
        # Heuristics:
        # 1. How many variables would this KS's output affect?
        # 2. How long since it last ran?
        # 3. Is there a domain that is "almost solved" that this KS targets?
        # 4. Cost-benefit: runtime vs. expected domain reduction
        
        impact = ks.estimated_impact(blackboard)
        recency = time_since_last_run(ks)
        benefit_cost_ratio = impact / ks.estimated_cost()
        
        return 0.4 * impact + 0.3 * benefit_cost_ratio + 0.3 * recency
    
    def select_next_ks(self, ready_ks, blackboard):
        rated = [(self.rate_ks(ks, bb), ks) for ks in ready_ks]
        return max(rated, key=lambda x: x[0])[1]
```

### 7.3 Support Focus of Attention

For large problems, narrow the focus:
```python
class FocusOfAttention:
    def set_focus(self, region):
        """Focus on a subset of variables/constraints."""
        self.focus_variables = region.variables
        self.focus_constraints = region.constraints
    
    def ready_knowledge_sources(self):
        """Only evaluate KSs relevant to the focus region."""
        return [ks for ks in self.all_ks 
                if ks.relevant_to_focus(self.focus)]
```

### 7.4 Implement Control-Level Reasoning (BB1 Style)

```python
class ControlKS(KnowledgeSource):
    """Meta-level reasoning about solving strategy."""
    
    def triggered(self, bb):
        return bb.no_progress_for(iterations=1000) or bb.backtracking_loop_detected()
    
    def run(self, bb):
        if bb.backtracking_too_much():
            bb.suggest_restart()
        elif bb.stuck_on_hard_constraint():
            bb.relax_constraint_temporarily()
        else:
            bb.switch_search_strategy("randomized")
```

### 7.5 Make Triggering Conditions Declarative

```python
# Instead of imperative "if" checks, declare triggers:

@knowledge_source(
    trigger="""
        has_constraint_of_type(SetMembership) AND
        member_domain_size(member) < threshold
    """
)
class SetMembershipPropagationKS:
    def run(self, bb):
        ...
```

---

## Part 8: Lessons from Blackboard History

### 8.1 The Problem of Control Complexity

Early blackboard systems showed that even with a clean architecture, control complexity grows. HEARSAY-II had to carefully tune priorities to balance specialist contributions. BB1's solution—apply the blackboard model to control itself—was profound: treat control decisions as hypotheses subject to the same opportunistic reasoning as domain decisions.

**Lesson for Evident**: As the runtime grows, maintain a clean separation between domain-level solving (constraint propagation, search) and control-level decisions (which strategy to pursue). Both can be expressed as hypotheses on the blackboard.

### 8.2 The Importance of Transparency

HEARSAY-II's success partly stemmed from the blackboard being inspectable. Researchers could see why the system made a choice (what was on the blackboard, which KS fired, what did it change?). This transparency enabled debugging and improvement.

**Lesson for Evident**: The blackboard (solver state) should be observable. Provide query results in the form of derivation trees or justifications ("variable x was narrowed to {1,3,5} because of constraint C and KS K").

### 8.3 Scalability via Efficient Pattern Matching

GBB improved on HEARSAY-II by making pattern matching faster (indexing, caching). As the blackboard grows, checking triggering conditions naively becomes slow.

**Lesson for Evident**: Efficient matching of KS triggers is critical. Index the blackboard by constraint type, variable, etc., so the controller can quickly find ready KSs without scanning the whole state.

### 8.4 The Value of Domain-Specific Heuristics

Blackboard systems are not general-purpose. Their effectiveness depends on domain-specific heuristics for:
- Triggering conditions (when can a KS usefully act?)
- Priorities (which KS should act first?)
- Focus of attention (where is effort best spent?)

**Lesson for Evident**: The runtime should expose hooks for domain-specific heuristics. A user solving scheduling problems might want to prioritize constraints on the critical path; a user solving combinatorial problems might want to order variables by domain size.

---

## Part 9: Integration with Evident's Current Architecture

Evident's runtime pipeline is:

```
source → normalizer → parser → transformer → AST 
  → sorts → instantiate → translate → evaluate → runtime
```

The **evaluate** stage (running Z3) is where solving happens. A blackboard view would refactor `evaluate` and `runtime`:

### 9.1 Current `evaluate.py`

```python
class EvidentSolver:
    def run(self, z3_solver, goals):
        result = z3_solver.check()
        if result == sat:
            return extract_model(...)
        else:
            return unsat / unknown
```

Z3 is a black box; Evident calls it and gets a result.

### 9.2 Blackboard-Refactored Version

```python
class ConstraintBlackboard:
    def __init__(self):
        self.variables = {}  # var_name -> domain
        self.constraints = []  # list of constraints
        self.partial_assignment = {}  # var_name -> value
        self.history = []  # for backtracking

class EvidentSolverWithBlackboard:
    def __init__(self):
        self.blackboard = ConstraintBlackboard()
        self.knowledge_sources = [
            ArcConsistencyKS(),
            SetMembershipKS(),
            SearchKS(),
            BacktrackKS(),
            ControlKS(),
        ]
        self.scheduler = Scheduler()
    
    def run(self, goals):
        while not done(self.blackboard):
            ready = [ks for ks in self.knowledge_sources 
                     if ks.triggered(self.blackboard)]
            if not ready:
                break  # Stalled
            next_ks = self.scheduler.select_next_ks(ready, self.blackboard)
            next_ks.run(self.blackboard)
        
        return self.blackboard.partial_assignment
```

This refactoring:
1. Exposes the solving process as a sequence of KS firings.
2. Allows users to add custom KSs.
3. Enables tracing (debugging why a choice was made).
4. Opens the door to distributed solving (blackboard as a shared service).

---

## Part 10: Future Directions

### 10.1 Adaptive Solving

A meta-level blackboard could learn which strategies work best on different problem types:
- For scheduling problems: use constraint-specific propagation heavily.
- For combinatorial optimization: use local search after exhausting propagation.
- For large graphs: use iterative refinement.

### 10.2 Collaborative Solving

Multiple solvers (possibly different engines—Z3, CLP, local search) contribute to a shared blackboard, each offering partial solutions. A meta-level KS synthesizes the best insights.

### 10.3 Interactive Problem-Solving

A user iteratively refines a specification:
1. Post initial constraints.
2. Get a solution.
3. Add a new constraint (e.g., "cost < 100").
4. The solver re-prioritizes KSs, re-evaluates with focus on the new constraint.
5. Get a refined solution without re-solving from scratch.

This is more efficient than existing incremental solvers and aligns with opportunistic reasoning.

### 10.4 Constraint Relaxation and Approximate Solutions

If a full solution is infeasible, a meta-level KS could:
- Relax lower-priority constraints.
- Use local search to find approximate solutions.
- Provide a "best-effort" result with an explanation of which constraints were violated.

---

## Conclusion

The blackboard architecture is not a novel pattern; it has been proven over 50 years in diverse domains. Its strength lies in **modular opportunistic reasoning**: specialists act based on opportunity (current state), not predetermined order, enabling flexible, adaptive problem-solving.

For Evident, adopting blackboard principles would:

1. **Clarify control**: Make the solver's decision-making transparent and modular.
2. **Enable extensibility**: Users can add custom solving strategies as KSs.
3. **Improve adaptability**: The system can dynamically switch strategies based on problem characteristics.
4. **Support distribution**: A distributed blackboard opens the door to parallel and remote solving.
5. **Enhance explainability**: Each decision is traceable to a KS firing and blackboard state.

The core insight is this: **Constraint solving is not a monolithic algorithm, but a coordination problem—multiple specialists (constraint types, propagation rules, search strategies) must cooperate on a shared state. The blackboard architecture is the proven pattern for such cooperation.**

---

## References

### Foundational Papers

- [The Blackboard Model of Problem Solving and the Evolution of Blackboard Architectures](https://ojs.aaai.org/aimagazine/index.php/aimagazine/article/view/537) — Nii (1986), AI Magazine. Comprehensive overview of the pattern and its evolution.

- [The Hearsay-II Speech-Understanding System: Integrating Knowledge to Resolve Uncertainty](https://websites.nku.edu/~foxr/CSC425/hearsay2.pdf) — Erman et al. (1980). The foundational application.

- [A Retrospective View of the Hearsay-II Architecture](https://dl.acm.org/doi/abs/10.5555/1622943.1623004) — Proceedings of IJCAI 1978.

- [Focus of Attention in the HEARSAY II Speech Understanding System](https://www.researchgate.net/publication/220814833_Focus_of_attention_in_the_HEARSAY_II_speech_understanding_system) — Lesser & Erman (1977). Deep dive on focus mechanisms.

- [BB1: An Architecture for Blackboard Systems that Control, Explain, and Learn](http://i.stanford.edu/pub/cstr/reports/cs/tr/84/1034/CS-TR-84-1034.pdf) — Hayes-Roth et al. (1984). Meta-level reasoning in blackboard systems.

### Modern Applications

- [Using Constraint Propagation in Blackboard Systems: A Flexible Software Architecture for Reactive and Distributed Systems](https://ieeexplore.ieee.org/document/144397) — IEEE. Integration of constraint propagation with blackboard architecture.

- [Blackboard Architecture for Reactive Scheduling](https://www.academia.edu/8039111/Blackboard_architecture_for_reactive_scheduling) — Manufacturing and process control applications.

- [Building Intelligent Multi-Agent Systems with MCPs and the Blackboard Pattern](https://medium.com/@dp2580/building-intelligent-multi-agent-systems-with-mcps-and-blackboard-pattern-to-build-systems-a454705d5672) — Denis Petelin (2024). Modern perspective on blackboard + multi-agent systems.

- [Four Design Patterns for Event-Driven, Multi-Agent Systems](https://www.confluent.io/blog/event-driven-multi-agent-systems/) — Confluent. Blackboard in distributed, event-driven contexts.

### Pattern Comparisons

- [Blackboard (Design Pattern)](https://en.wikipedia.org/wiki/Blackboard_(design_pattern)) — Wikipedia overview and comparison to other patterns.

- [Architectural Patterns: Blackboard, Publish–Subscribe, and Proxy/Broker](https://medium.com/@ilakshitha7921/architectural-patterns-in-software-engineering-blackboard-publish-subscribe-and-proxy-broker-465638aa30b8) — Ishan Lakshitha. Clear comparisons.

---

**Document prepared for**: Evident constraint programming language design.

**Focus**: Applying blackboard architecture principles to coordinate multiple constraint-solving strategies and enable opportunistic, adaptive problem-solving.
