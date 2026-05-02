# Belief Propagation and Message-Passing Algorithms: A Research Document for Constraint Programming Languages

## Executive Summary

Belief propagation (BP) and message-passing algorithms are iterative, distributed techniques for solving inference and constraint satisfaction problems on graphical models. They operate by having nodes in a network repeatedly exchange "messages" (beliefs or constraints) with neighbors until reaching a convergence fixpoint. This document surveys the theoretical foundations of these algorithms, their practical applications, and their relevance to Evident—a constraint programming language that may benefit from understanding how multiple constraint schemas can "ping-pong" outputs to each other.

The key insight: BP's convergence properties and fixpoint theory provide a framework for understanding when and why such iterative constraint systems converge, what guarantees we can make, and where they fail.

---

## 1. What is Belief Propagation? Foundations and Terminology

### 1.1 Core Definition

Belief propagation is a message-passing algorithm for performing inference on graphical models, such as Bayesian networks and Markov random fields (MRFs). Its goal is to compute marginal distributions for variables in a probabilistic model conditioned on observed data.

The algorithm computes beliefs (posterior probability distributions) at each node by passing messages from nodes to neighbors iteratively. When the graph is a tree (acyclic), this process converges exactly to the true marginals. When the graph contains cycles (loopy BP), the algorithm may only approximate the true marginals or may not converge at all.

### 1.2 The Sum-Product Algorithm

Belief propagation is also known as the **sum-product algorithm** because marginalization of a joint probability distribution is implemented as a sum of products of factors:

```
P(X_i) = Σ_{all other variables} ∏_{factors} f_a(X_a)
```

Rather than computing this product explicitly (which is exponential), the algorithm factors the computation across the network, computing partial products at each node and passing them as messages.

**Message Update Rule** (simplified):

Each node sends a message to a neighbor that is proportional to the product of:
- Its local factor (if it is a factor node)
- Messages received from all other neighbors

This local computation propagates information through the network without requiring global enumeration of all assignments.

### 1.3 Historical Context

- **Invented**: Judea Pearl formulated belief propagation in 1982 as an exact inference algorithm for polytrees (graphs with at most one path between any two nodes).
- **Extended**: Later research extended it to general graphs (loopy BP), recognizing that while no longer exact, it often produces useful approximations.
- **Empirical Success**: BP has been remarkably successful in applications like error-correcting codes, image processing, and satisfiability solving—despite lack of convergence guarantees on loopy graphs.

---

## 2. Graphical Models: Bayesian Networks, Markov Random Fields, and Factor Graphs

To understand belief propagation, we must understand the graphical models it operates on.

### 2.1 Bayesian Networks

A Bayesian network is a directed acyclic graph (DAG) where:
- Nodes represent random variables
- Directed edges represent conditional dependencies
- Each node has a conditional probability table (CPT) given its parents

**Example**: A medical diagnosis network might have disease nodes pointing to symptom nodes, encoding the idea that disease causes symptoms.

**Inference Problem**: Given observations (e.g., patient has symptom S), compute P(disease | S).

### 2.2 Markov Random Fields (MRFs)

An MRF is an undirected graphical model where:
- Nodes are variables
- Edges represent interactions (dependencies)
- Joint probability factors over maximal cliques (complete subgraphs)

**Key Difference from Bayes Nets**: MRFs are undirected and allow cycles. They model symmetric relationships (e.g., spatial smoothness in image segmentation).

### 2.3 Factor Graphs

A factor graph is a bipartite graph with two types of nodes:
- **Variable nodes** (circles): represent unknown quantities
- **Factor nodes** (squares): represent constraints or local functions

**Edges** connect variables to factors they appear in.

**Relationship to Constraint Programming**:
- A factor graph is essentially a visual representation of a constraint satisfaction problem (CSP).
- Variable nodes are domains.
- Factor nodes are constraints.
- The sum-product algorithm on factor graphs is equivalent to the max-product algorithm for finding the most likely assignment—which is analogous to solving a CSP.

**Constraint Graphs**: A constraint graph is a factor graph where all factors are hard constraints (satisfiability). The max-product algorithm becomes arc-consistency.

### 2.4 Unifying View

All three graphical models can be expressed as factor graphs:
- Bayes nets: convert directed edges to factors
- MRFs: factors over cliques
- CSPs: constraint factors

---

## 3. The Message-Passing Pattern: The Ping-Pong Loop

### 3.1 Generic Message-Passing Procedure

```
Initialize all messages to 1 (or neutral)
Repeat until convergence:
  For each edge (u → v) in the graph:
    Compute message from u to v based on:
      - Local factor at u
      - Messages received by u from all neighbors except v
    Update message(u → v)
  
  Check if any message changed significantly:
    If no, convergence reached → STOP
    Else, continue iteration
```

### 3.2 Convergence Criteria

Several convergence criteria are used in practice:

1. **No Change**: Messages don't change (or change below a threshold) on an iteration.
2. **Fixed Iteration Count**: Stop after N iterations (common in real applications to avoid infinite loops).
3. **Damping**: Messages are averaged with previous values: `m_new = α * m_computed + (1 - α) * m_old`, where α ∈ (0, 1]. Damping improves convergence on loopy graphs.
4. **Belief Convergence**: Compute node beliefs as the product of incoming messages; stop when beliefs stabilize.

### 3.3 Synchronous vs. Asynchronous Updates

- **Synchronous**: All messages update simultaneously in each iteration (classic BP).
- **Asynchronous (Residual)**: Messages that changed significantly in the previous iteration are updated with higher priority.

**Residual methods** can dramatically improve convergence speed and have shown better convergence on loopy graphs.

---

## 4. Exact vs. Loopy Belief Propagation

### 4.1 Trees (Acyclic Graphs): Convergence is Guaranteed

**Theorem**: On a tree (acyclic factor graph), belief propagation converges to exact marginal distributions in finite time, typically **2 iterations** with proper scheduling (one pass down the tree, one back up).

**Why**: Without cycles, there is exactly one path between any two nodes. A message from node A to node B carries all information from A's subtree. No information "loops back" to create cycles of messages.

**Scheduling**: With tree-structured graphs, we can schedule updates to minimize iterations:
- Root the tree at an arbitrary node
- Forward pass: update all nodes top-down
- Backward pass: update all nodes bottom-up
- After two full passes, all marginals are exact

### 4.2 Loopy Graphs: Convergence is Problematic

**Reality**: Real-world problems often have loops. Image segmentation, error-correcting codes, SAT solving—all have cyclic constraint graphs.

**Key Facts about Loopy BP**:

1. **Not Always Exact**: Beliefs computed may not be true marginals, even at convergence.
2. **May Not Converge**: Messages can oscillate indefinitely, with no stable fixpoint.
3. **Multiple Fixpoints**: When LP does converge, there may be multiple stable fixpoints, and the algorithm may reach different ones depending on initialization.
4. **Empirically Works Well**: Despite these issues, LBP often produces excellent approximate marginals and is widely used in practice.

**Sufficient Conditions for Convergence**:

Research has identified several conditions that guarantee or strongly encourage convergence:

- **Single Loop**: Graphs with exactly one cycle typically converge.
- **Weak Interaction Strength**: If factors/constraints are "soft" (not hard constraints), messages remain bounded and convergence is more likely.
- **Tree-width**: Graphs with low tree-width converge better.
- **Contractivity**: If the message update operator is a contraction mapping (messages change less at each step), fixpoint exists.
- **Spectral Gap**: Graphs with spectral gaps in their adjacency matrix converge better.

**Necessary Conditions**: No simple necessary and sufficient conditions are known for arbitrary graphs.

**Practical Guidance**:
- On loopy graphs, use damping or residual updates.
- Set iteration limits to avoid infinite loops.
- Monitor convergence heuristically (e.g., max message change per iteration).
- Use multiple random initializations and accept the best result.

---

## 5. Convergence Theory: The Fixpoint Perspective

### 5.1 Message Passing as Fixpoint Computation

Belief propagation can be formalized as finding a fixpoint of the message-update function:

```
M^{(t+1)} = F(M^{(t)})
```

where:
- M^{(t)} is the vector of all messages at iteration t
- F is the operator that computes new messages from old ones
- A convergence fixpoint satisfies: M* = F(M*)

### 5.2 The Knaster-Tarski Fixed-Point Theorem

The Knaster-Tarski theorem is a fundamental result guaranteeing fixpoint existence:

**Theorem**: For a complete lattice L and a monotone (order-preserving) function f: L → L, the set of fixpoints of f forms a complete lattice.

Moreover, by iteration, we can compute:
- Least fixpoint: lim_{n→∞} f^n(⊥) (starting from bottom)
- Greatest fixpoint: lim_{n→∞} f^n(⊤) (starting from top)

**Application to BP**:
- The message space forms a lattice (ordered by element-wise comparison).
- If the message-update operator F is monotone, then a fixpoint exists.
- Repeated application F^n (starting from neutral messages) will converge to the least fixpoint.
- **Key Limitation**: Convergence may be slow, and reaching a least fixpoint doesn't guarantee correctness for loopy graphs.

### 5.3 Contraction Mapping Principle

A stronger result applies when F is a **contraction mapping**:

**Definition**: A function F on a metric space is a contraction if there exists k < 1 such that:
```
d(F(x), F(y)) ≤ k · d(x, y) for all x, y
```

**Theorem (Banach)**: Every contraction mapping on a complete metric space has a unique fixpoint, and iteration converges exponentially fast.

**For BP**: Under certain conditions (weak factors, bounded graph degree, damping), the message-update operator can be shown to be a contraction. This gives:
- Unique fixpoint (no oscillation or multiple attractors)
- Exponential convergence rate
- Robustness to initialization

**In Practice**: Damping (α-weighted averaging with previous messages) increases the contraction coefficient and improves convergence.

---

## 6. Survey Propagation: BP for SAT Solving

### 6.1 The Problem: SAT Solving at the Threshold

Random k-SAT instances become increasingly hard as the clause-to-variable ratio (constraint density) approaches a critical threshold. Near this threshold:
- DPLL-like solvers slow exponentially
- The solution space transitions from a single cluster to exponentially many clusters
- Traditional algorithms struggle

### 6.2 Survey Propagation: Adapting BP for SAT

Survey propagation (SP) is a generalization of BP that computes "surveys" over clusters of solutions rather than individual beliefs.

**Key Insight**: Instead of messages being marginal probabilities over single variables, SP messages encode surveys—distributions over possible messages from other nodes.

**How It Works**:
1. Represent the k-SAT formula as a factor graph: variable nodes and clause nodes.
2. In BP on this graph, a message from a clause to a variable encodes the clause's "opinion" about the variable.
3. In SP, the message from a clause becomes a distribution over possible opinions—a "survey."
4. Iterate until convergence, then extract a satisfying assignment.

**Empirical Effectiveness**: SP has been remarkably effective for random k-SAT instances near the threshold, solving problems that DPLL fails on. This was a major discovery in the SAT solving community around 2000.

### 6.3 Why SP Works Better Than BP

At the satisfiability threshold, the constraint landscape is highly non-convex with exponentially many solutions in distant clusters. BP's assumption of a single solution cluster breaks down.

SP's survey mechanism allows it to implicitly represent the multi-cluster structure. By reasoning about distributions over messages (meta-messages), SP explores different clusters and finds solutions more easily.

### 6.4 Relationship Between SP and BP

- **BP on Uniform Distribution**: Standard BP on the uniform distribution over satisfying assignments.
- **SP**: BP on a different distribution that reweights solution clusters.
- **Continuous Range**: One can interpolate between BP and SP by varying the distribution, recovering both as extreme points.

---

## 7. Connection to DPLL(T) and Modern SMT Solvers

### 7.1 DPLL Algorithm

DPLL (Davis-Putnam-Logemann-Loveland) is the foundational algorithm for SAT solving:

1. **Unit Propagation**: If a clause has only one unassigned literal, assign it to satisfy the clause.
2. **Pure Literal Elimination**: If a literal appears only positive (or only negative), assign it accordingly.
3. **Search**: Pick an unassigned variable, try assigning it true, recursively solve; if unsatisfiable, backtrack and try false.

DPLL is essentially a constraint propagation algorithm—each node propagates its constraints to neighbors.

### 7.2 DPLL(T): Extending to First-Order Theories

DPLL(T) is an architecture for SAT Modulo Theories (SMT) solving that combines:
- **SAT Solver**: Handles Boolean reasoning and search
- **Theory Solver(s)**: Handles domain-specific constraints (linear arithmetic, arrays, bit-vectors, etc.)

**Message Passing Between SAT and Theory Solvers**:

This is where message passing becomes explicit:

1. **SAT Solver** proposes a Boolean assignment (a partial truth assignment to atoms).
2. **Theory Solver** checks if the assignment is consistent with theory constraints. If yes, the theory solver propagates additional inferences back to the SAT solver (new atoms that must be true/false).
3. **SAT Solver** incorporates these propagations and continues search.
4. If the theory solver detects a conflict, it sends an explanation back to the SAT solver.
5. Both solvers iterate (ping-pong) until finding a satisfying assignment or proving unsatisfiability.

**Relevancy Propagation in Z3**: Modern solvers like Z3 use relevancy analysis to avoid propagating irrelevant atoms, reducing message volume and improving efficiency.

### 7.3 Analogy to Belief Propagation

- **BP Message**: A node sends its local belief about a variable to a neighbor.
- **DPLL(T) Message**: The SAT solver propagates an atom assignment to the theory solver; the theory solver propagates inferred atoms back.
- **Convergence**: BP converges when messages stabilize; DPLL(T) "converges" when search terminates (no more conflicts detected).

**Key Difference**: DPLL(T) is a deterministic search algorithm with backtracking, while BP is inherently iterative and non-backtracking. However, both involve iterative message exchange until a fixpoint is reached.

---

## 8. Practical Applications: Examples of BP in Action

### 8.1 LDPC Codes: Error-Correcting via Belief Propagation

**Low-Density Parity-Check (LDPC) Codes**: Linear error-correcting codes with sparse parity-check matrices, used in modern communications (Wi-Fi, 5G, DVB-S2).

**Why BP Works for LDPC**:

1. **Factor Graph Structure**: A parity-check code naturally maps to a factor graph with variable nodes (bits) and check nodes (parity checks).
2. **Message Interpretation**: Messages represent log-likelihood ratios (LLRs) of bit values based on received channel measurements and neighboring checks.
3. **Iterative Decoding**: Compute bits by passing messages between variable and check nodes.
4. **Convergence**: With proper code design (careful graph structure), BP converges in ~100 iterations even for long codes.

**Performance**: LDPC codes can achieve error rates approaching channel capacity (Shannon limit) at practical block lengths, thanks to BP decoding's efficiency.

**Key Insight**: BP's convergence is good for LDPC because the underlying factor graphs are carefully designed to have low cycle girth (long shortest cycle), reducing loops and improving convergence.

### 8.2 Image Segmentation via Markov Random Fields

**Problem**: Label each pixel in an image (e.g., foreground/background, semantic segmentation).

**MRF Model**:
- Variable nodes: pixel labels
- Factors: data term (image evidence at each pixel) and smoothness term (neighboring pixels should have similar labels)

**Loopy BP Application**:

1. Initialize messages with data evidence.
2. Iterate: variable nodes aggregate messages from neighboring factor nodes; factor nodes aggregate messages from neighboring variable nodes.
3. After convergence, each pixel's label is determined by its beliefs (max product or marginal).

**Challenges**:
- Factor graphs contain many loops (image pixels form a grid).
- Label spaces can be large (hundreds of semantic classes).
- Convergence is not guaranteed.

**Solutions**:
- Use damping and residual updates.
- Prune unlikely labels (adaptive label pruning).
- Use hierarchical (multi-scale) inference: solve at coarse scales, refine at fine scales (pyramid structure).

**Success**: LBP for image segmentation often produces quality results competitive with more expensive methods like graph cuts, especially on large images where search-based methods are too slow.

### 8.3 Protein Folding and Bioinformatics

Belief propagation has been applied to inference problems in protein structure prediction and phylogenetics. Here, the graphical model represents:
- Variables: amino acid types or structural properties
- Factors: sequence homology, structural constraints, evolutionary relationships

BP allows efficient inference over high-dimensional spaces that would be intractable with exhaustive search.

---

## 9. Evidently Relevant: Applying BP Theory to Evident's Constraint Schemas

Evident is a constraint programming language where:
- Schemas define sets via membership conditions.
- Queries ask whether a satisfying assignment exists.
- The runtime translates constraints to Z3 (an SMT solver).

A natural evolution might involve:
- Multiple schemas that reference each other (schema composition).
- Iterative refinement: one schema's query result feeds into another's query, which feeds back to the first.
- Goal: Schemas "ping-pong" outputs to each other until reaching a stable configuration.

### 9.1 Mapping BP Concepts to Evident

| BP Concept | Evident Analogue |
|---|---|
| Factor graph | Schema dependency graph |
| Variable node | Schema variable binding |
| Factor node | Constraint expression |
| Message | Derived output from one schema's query (e.g., inferred set membership) |
| Convergence | All schemas reach a fixpoint: outputs don't change on further iterations |
| Loopy graph | Cyclic schema dependencies (A uses B, B uses A) |

### 9.2 Convergence Guarantees for Iterative Schemas

**Applying Knaster-Tarski**:

1. **Define a Lattice**: The state space is the Cartesian product of output spaces for all schemas. Order elements point-wise: configuration A ≤ B if each schema's output in A is a subset of its output in B (lattice of sets).

2. **Define the Update Operator**: F(config) computes the new outputs by running each schema's query with inputs from the current configuration.

3. **Monotonicity**: If schemas are monotone (larger inputs produce larger outputs), then F is monotone. By Knaster-Tarski, a fixpoint exists.

4. **Convergence**: Repeatedly applying F converges to the least fixpoint. On a finite lattice (finite output spaces), this terminates in finite iterations.

**Example**: 
```
Schema A(x ∈ Set_B):
  # Members of A are those whose type matches members of B
  x ∈ {y | type(y) ∈ B}

Schema B(x ∈ Set_A):
  # Members of B are those whose size is smaller than some member of A
  x ∈ {y | size(y) < max(size(a) for a ∈ A)}
```

Iteration:
- Iteration 0: Set_A = ∅, Set_B = ∅
- Iteration 1: Set_A = ∅ (no members, empty input), Set_B = ∅
- Iteration 2: No change
- Fixpoint reached at iteration 1.

Alternatively, with richer data, the fixpoint might stabilize at iteration 3 after progressively including more members.

### 9.3 When Fixpoints Fail: Non-Monotone Schemas

**Problem**: If a schema is non-monotone (e.g., uses negation), monotonicity fails.

```
Schema A(x ∈ Set_B):
  x ∈ {y | y ∉ B}  # Complement of B
```

Now F is not monotone (larger Set_B → smaller Set_A). Knaster-Tarski doesn't apply.

**Consequences**:
- Fixpoint may not exist (e.g., A = ¬B and B = ¬A lead to oscillation).
- If a fixpoint exists, iteration may not reach it; the system may cycle.
- Different initializations may converge to different "attractors."

**Solutions**:
- Use negation-as-failure (NAF) carefully; require stratification (no negative cycles).
- Apply the fixpoint as the least fixpoint of the positive core (ignore negation in convergence).
- Detect cycles and reject non-stratified queries.

### 9.4 Efficiency: Speed of Convergence

From BP theory, several factors affect convergence speed:

1. **Graph Structure**: Acyclic schema dependencies converge in 2 passes (one forward, one backward, like tree BP). Cyclic dependencies require more iterations.

2. **Constraint Strength**: Tight constraints (narrow factor domains) converge faster because there's less ambiguity.

3. **Damping**: If iterative computations exhibit oscillation, apply damping: `output_new = α * output_computed + (1 - α) * output_old`.

4. **Residual Updates**: Prioritize updating schemas whose outputs changed significantly, reducing wasted iterations.

5. **Asymptotic Behavior**: On a finite lattice, convergence is guaranteed monotone, but may require O(|lattice|) iterations in the worst case (exponential in number of variables).

---

## 10. What Makes Ping-Pong Systems Tractable or Intractable

### 10.1 Tractability Conditions

A constraint system that iterates to fixpoint is tractable if:

1. **Finite Lattice**: The output space is finite (e.g., finite sets, bounded integers). Then iteration must terminate.

2. **Monotonicity**: The update operator is monotone. Combined with (1), this guarantees convergence to a least fixpoint.

3. **Low Tree-Width**: The schema dependency graph has low tree-width. This makes inference efficient (tree-width determines complexity in constraint systems, similar to BP).

4. **Weak Coupling**: Schemas are loosely coupled (few dependencies). Strong coupling increases iteration count.

5. **Natural Damping**: Negative feedback in constraints naturally dampens oscillation (e.g., increasing A decreases B, which increases C but less so than before).

### 10.2 Intractability Conditions

Iteration becomes intractable or problematic if:

1. **Infinite Lattice**: Output spaces are unbounded (e.g., real numbers, unbounded integers). Iteration may not terminate.
   - **Example**: Temperature T_A ← k * T_B, T_B ← m * T_A with k, m > 1 → both grow indefinitely.

2. **Non-Monotonicity**: Negation or complex non-monotone operations cause oscillation or multiple attractors.
   - **Example**: A = ¬B, B = ¬A → no fixpoint (A and B flip forever).

3. **High Tree-Width**: Schema graph is dense (e.g., all-to-all dependencies). Each iteration must consider all interactions, becoming expensive.

4. **Tight Cycles**: Short feedback loops (A → B → A) with strong constraints magnify changes, delaying convergence.

5. **Exponential Blowup**: Output space grows exponentially with iteration (e.g., powerset of unbounded domain). Fixpoint may not be representable finitely.

### 10.3 Design Recommendations for Evident

To ensure iterative schema systems remain tractable:

1. **Restrict Negation**: Use negation-as-failure with stratification checks. Reject programs with cycles through negation.

2. **Enforce Monotonicity**: Encourage (or require) schemas to be monotone in their inputs. Document where this breaks.

3. **Bound Output Spaces**: Ensure all outputs are finite-sized (e.g., constrain set size, integer bounds). If Z3 is involved, these bounds translate to SMT constraints.

4. **Provide Iteration Limits**: Allow users to specify max iterations. If reached without convergence, flag the non-convergence or return an approximation.

5. **Detect Cycles**: Analyze the schema dependency graph for cycles. Warn users; use damping or require stronger convergence checks for cycles.

6. **Offer Residual Updates**: Implement residual message passing: prioritize computing schemas whose inputs changed significantly, reducing wasted iterations.

7. **Monitor Convergence**: At each iteration, track how much outputs change. Declare convergence when changes are below a threshold (absolute or relative).

8. **Document Performance**: Provide timing information and iteration counts in results. Help users understand why certain programs are slow.

---

## 11. Summary: Belief Propagation as a Lens for Constraint Programming

### 11.1 Key Takeaways

1. **Belief Propagation is Message Passing**: Nodes (or constraints) iteratively communicate with neighbors until reaching a stable state (convergence).

2. **Convergence is Theoretical**: On acyclic graphs (trees), BP converges exactly in finite iterations. On loopy graphs, convergence is not guaranteed but often works empirically.

3. **Fixpoint Theory Explains Why**: Knaster-Tarski and contraction mapping theorems guarantee fixpoint existence under monotonicity. Iteration converges by applying the update operator repeatedly.

4. **SAT/SMT Solvers Use Message Passing**: DPLL(T) systems pass messages between SAT and theory solvers, analogous to BP nodes passing beliefs.

5. **Practical Applications Flourish**: LDPC codes, image segmentation, and protein folding demonstrate that even loopy BP produces useful results in real problems.

6. **Evident Can Leverage This**: Iterative schema composition in Evident can be understood through the BP and fixpoint lens. Proper analysis ensures tractability.

### 11.2 For Evident's Design

When designing iterative schema composition:
- Prove monotonicity to guarantee convergence.
- Monitor for non-monotone patterns (negation, complex feedback).
- Use finite output spaces to ensure termination.
- Detect and handle cycles gracefully.
- Provide transparency: report iteration counts and convergence status.
- Document which schema programs are expected to be fast vs. slow.

The ping-pong pattern of constraint schemas is not a novel idea—it's a well-studied pattern in constraint propagation, belief propagation, and SMT solving. Understanding this theory prevents re-inventing solutions and provides a roadmap for addressing challenges.

---

## 12. References and Further Reading

### Primary Sources

- **Belief Propagation Foundations**: 
  - Pearl, J. (1982). "Reverend Bayes on Inference Engines: A Distributed Hierarchical Approach." 
  - Kschischang, F. R., Frey, B. J., & Loeliger, H. A. (2001). "Factor graphs and the sum-product algorithm." IEEE Transactions on Information Theory.

- **Loopy Belief Propagation**:
  - Mooij, J. M. (2011). "Understanding and Improving Belief Propagation."
  - CMU Lecture Notes on Loopy BP: https://www.cs.cmu.edu/~epxing/Class/10708-14/scribe_notes/scribe_note_lecture13.pdf

- **Survey Propagation**:
  - Mézard, M., Parisi, G., & Zecchina, R. (2002). "Analytic and algorithmic solution of random satisfiability problems." Science.
  - Maneva, E., et al. (2005). "A New Look at Survey Propagation and its Generalizations."

- **DPLL(T) and SMT**:
  - Tinelli, C., & Barrett, C. (2007). "Satisfiability Modulo Theories." 
  - Moura, L. de, & Bjørner, N. (2008). "Z3: An Efficient SMT Solver." TACAS.

- **Fixpoint Theory**:
  - Knaster-Tarski Theorem: https://en.wikipedia.org/wiki/Knaster%E2%80%93Tarski_theorem
  - Cousot, P. "Mathematical Foundations: (5) Fixpoint Theory." MIT lecture notes.

- **Constraint Programming**:
  - Dechter, R. (2003). Constraint Processing. Morgan Kaufmann.
  - Barták, R. "Constraint Guide: Consistency Techniques." https://ktiml.mff.cuni.cz/~bartak/constraints/consistent.html

### Online Resources

- **Wikipedia on Belief Propagation**: https://en.wikipedia.org/wiki/Belief_propagation
- **Rylan Schaeffer's Guide**: https://rylanschaeffer.github.io/content/learning/probabilistic_graphical_models/exact_inference_algs/belief_propagation.html
- **Factor Graphs Tutorial**: https://www.isiweb.ee.ethz.ch/papers/arch/aloe-2004-spmagffg.pdf
- **LDPC Decoding**: https://yair-mz.medium.com/decoding-ldpc-codes-with-belief-propagation-43c859f4276d
- **Arc Consistency in CSPs**: https://www.cs.ubc.ca/~mack/CS322/lectures/3-CSP3.pdf

---

## Appendix: Worked Example — Iterative Schema Ping-Pong

Suppose we have two schemas in a hypothetical Evident program:

```
schema Person:
  age ∈ Nat
  name ∈ String

schema Adult(p ∈ Person):
  p ∈ {x | x.age ≥ 18}

schema Team(a ∈ Adult):
  a ∈ {x | x.name matches team_pattern}
```

**Execution Path**:

1. Query: `?Person p, Adult a ∈ p, Team t ∈ a`
2. **Iteration 0**: 
   - Person: All people in database (assume 100 records)
   - Adult: Filter to age ≥ 18 (assume 60)
   - Team: Filter by name pattern (assume 40)
   - Output: 40 team members
3. **Iteration 1** (if we feed Team results back to Adult):
   - If Adult now only considers inputs from Team (40 people)
   - Adult: Still filters to age ≥ 18; all 40 are already ≥ 18
   - No change
4. **Convergence**: Reached at iteration 1 (outputs stabilized).

This is a simple, acyclic case: Person → Adult → Team. Convergence is immediate.

**Harder Example: Cyclic Dependency**

```
schema Investor:
  portfolio_value ∈ Real

schema Company(i ∈ Investor):
  valuation ∈ {v | v = sum(portfolio_value for i ∈ Investor if owns_stock(i, self))}

schema Investor2(c ∈ Company):
  portfolio_value ∈ {v | v = sum(c.valuation for c ∈ Company if owns_stock(self, c))}
```

This creates a circular dependency: Investor2 depends on Company, which depends on Investor. 

**Iteration**:
- Iteration 0: Investor.portfolio_value = 0, Company.valuation = 0
- Iteration 1: Investor2 computes portfolio_value based on companies' valuations (all 0) → portfolio_value = 0. No change.
- Convergence at iteration 1 (fixpoint at all zeros).

But with actual data (non-zero initial values), the iteration might oscillate or converge to a non-zero fixpoint, depending on the exact values and functions.

---

This document provides a thorough foundation for understanding belief propagation, message-passing algorithms, and their applicability to iterative constraint systems like Evident.
