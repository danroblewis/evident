# Nelson-Oppen Theory Combination and SMT Solver Architecture

## Executive Summary

This document explores the Nelson-Oppen combination framework and SMT solver architecture, with emphasis on how Z3 internally combines theory solvers—and what that tells us about combining schemas in Evident. The key insight: Z3 is already performing multi-theory combination at the solver level. Evident can expose this same pattern at the language level, where multiple constraint schemas cooperate like theory solvers, sharing information about equalities on common variables.

---

## Part I: The Nelson-Oppen Combination Framework

### Historical Context

In 1979, Nelson and Oppen introduced a landmark theorem for combining decision procedures: given two decidable theories with **disjoint signatures** (non-overlapping function and predicate symbols), if both are **stably infinite**, then their union is decidable via a cooperative method that sends information back and forth between solvers.

This framework is the foundation of all modern SMT solvers.

### The Core Problem

When we have multiple theories—arithmetic, uninterpreted functions (EUF), arrays, strings, bit-vectors—we need a way to combine them into a single decision procedure. Nelson-Oppen solves this without requiring all solvers to understand all theories.

**Example**: Consider the formula:
```
(x < y) ∧ (f(x) = a) ∧ (f(y) = b) ∧ (a ≠ b)
```

Here, `x < y` belongs to linear arithmetic over integers. `f(x) = a` and `f(y) = b` belong to the theory of uninterpreted functions (EUF). The predicate `a ≠ b` belongs to both (equalities are shared).

**Without combination**: We'd need a single solver understanding both theories simultaneously.

**With combination**: The arithmetic solver reasons about `x < y`, while the EUF solver reasons about `f(x) = a` and `f(y) = b`. They cooperate via shared equalities over the common variables `a` and `b`.

### The Two Phases: Purification and Propagation

Nelson-Oppen operates in two phases:

#### Phase 1: Purification

**Goal**: Separate formulas into theory-specific parts by introducing fresh variables.

**Input formula**:
```
f(x + 1) = a
```

**Purified form**:
```
y = x + 1  (arithmetic theory)
f(y) = a   (EUF theory)
```

We introduce a fresh variable `y` and flatten the nested term so each solver sees only its own theory's constructs.

#### Phase 2: Propagation

Once purified, the solvers alternate:

1. **Arithmetic solver** determines all satisfying assignments to the arithmetic constraints
2. **EUF solver** checks if the function equations are consistent with those assignments
3. If a solver finds a constraint, it reports it back
4. **Equalities on shared variables** are propagated bidirectionally

The process continues in a "ping-pong" fashion until either:
- Both solvers agree all constraints are satisfiable → **SAT**
- One solver reports unsatisfiability → **UNSAT**
- A contradiction emerges from shared equalities → **UNSAT**

### Stably Infinite Theories

**Definition**: A theory is **stably infinite** if every quantifier-free formula that is satisfiable in the theory has a satisfying interpretation with an infinite domain.

**Why it matters**: Nelson-Oppen requires this property to guarantee correctness. 

**Intuition**: If a theory can only be satisfied by finite domains (e.g., bit-vectors, finite datatypes), then forcing two theories into a joint model may introduce spurious constraints. Stably infiniteness ensures there's always "room" in the interpretation to satisfy both theories simultaneously.

**Examples**:
- **Stably infinite**: Linear arithmetic over reals, linear arithmetic over integers, EUF, strings
- **Not stably infinite**: Bit-vectors (finite domains), arrays with finite index/value sets

**Consequence**: Combining bit-vectors with stably infinite theories requires special handling (extended combination methods, discussed later).

### Disjoint Signatures Requirement

The theories must have **disjoint signatures**: they share no function symbols except equality `=`.

```
Theory A: +, -, ×, <, >   (arithmetic)
Theory B: f, g, h, ∈      (uninterpreted functions and custom relations)
Shared: =, ¬, ∧, ∨       (logical connectives and equality)
```

If both theories define their own `+` operator, Nelson-Oppen's method breaks—the combined semantics becomes ambiguous. This is why traditional SMT solvers **declare theories exhaustively upfront**: the solver architecture partitions the problem space into non-overlapping theory domains.

---

## Part II: The Equality-Sharing Protocol

At the heart of Nelson-Oppen is a simple but powerful idea: **theories communicate solely through shared equalities**.

### The Protocol

1. **Assertion**: Each theory solver receives a conjunction of literals (constraints)
2. **Local satisfaction**: Each solver checks if its constraints are satisfiable
3. **Shared variable extraction**: Identify all variables that appear in multiple theories
4. **Equality propagation**: 
   - If the EUF solver deduces `a = b` where `a` and `b` are shared, propagate this equality to all other solvers
   - If the arithmetic solver deduces `x = 3` where `x` is shared, propagate this to EUF
5. **Repeat until fixpoint**: Continue propagating until no new equalities are discovered
6. **Check arrangement**: If there's still ambiguity (disjunctions of equalities), test all possible arrangements

### Concrete Example: Arithmetic + EUF

**Formula**:
```
x < 5 ∧ y ≥ 5 ∧ f(x) = a ∧ f(y) = b ∧ a ≠ b
```

**Step 1: Purification**
- Already pure; no nesting

**Step 2: Send to solvers**

Arithmetic solver gets:
```
x < 5 ∧ y ≥ 5
```

EUF solver gets:
```
f(x) = a ∧ f(y) = b ∧ a ≠ b
```

Shared variables: `x`, `y`, `a`, `b`

**Step 3: Local checking**

- Arithmetic: Finds that `x ∈ [0, 4]` and `y ∈ [5, ∞)` satisfy the constraints
- EUF: Checks if `f(x) = a`, `f(y) = b`, `a ≠ b` can coexist

**Step 4: Propagation**

- Arithmetic doesn't deduce any equalities on `a` or `b` (they don't appear in its constraints)
- EUF deduces: Since `x < 5` and `y ≥ 5`, we have `x ≠ y`
  - Therefore, if `f` is a function, `f(x) ≠ f(y)` (otherwise `f` would be multi-valued)
  - So `a ≠ b` is forced by EUF

**Step 5: Conclusion**

Both solvers agree the formula is satisfiable with any assignment where `x ∈ [0, 4]`, `y ≥ 5`, `a` and `b` are distinct.

### Non-Convex Case Splitting

When one theory is **not convex** (e.g., integer linear arithmetic), the propagation protocol alone is insufficient. The solver must enumerate possible arrangements.

**Example**: 
```
(x = 0 ∨ x = 1) ∧ f(x) = a ∧ f(1) = b ∧ a ≠ b
```

Here, integer arithmetic forces a **disjunction**: `x` must be either 0 or 1. The EUF solver can't proceed until this is resolved.

**Solution**: The SAT/DPLL(T) layer enumerates both cases:
- **Case 1**: Assume `x = 0`, propagate, check for contradiction
- **Case 2**: Assume `x = 1`, propagate, check for contradiction

If both cases lead to UNSAT, the overall formula is UNSAT.

---

## Part III: DPLL(T) — Theory-Extended SAT Solving

The raw Nelson-Oppen protocol works for conjunctions of literals. To handle arbitrary Boolean formulas (disjunctions, implications, negations), SMT solvers combine it with a SAT solver via DPLL(T).

### Historical Context

**DPLL** (Davis-Putnam-Logemann-Loveland, 1962) is the core SAT solving algorithm. It:
1. Picks unassigned variables and assigns true/false
2. Simplifies the formula (unit propagation)
3. Backtracks on contradiction
4. Uses learned clauses to avoid repeated work (in CDCL variants)

**DPLL(T)** extends this by adding a theory solver at each decision point.

### The DPLL(T) Architecture

```
┌─────────────────────────────────────────────┐
│  SAT Solver (CDCL with Conflict Learning)   │
│  - Manages Boolean variables and clauses    │
│  - Performs unit propagation and backtrack  │
│  - Learns conflict clauses                  │
└────────────────┬────────────────────────────┘
                 │
         (assignments to atoms)
                 │
                 ↓
┌─────────────────────────────────────────────┐
│  Theory Solver (T-Solver)                   │
│  - Checks T-satisfiability of conjunctions  │
│  - Reports conflicts (unsatisfiable cores)  │
│  - Propagates consequences (theory lemmas)  │
└─────────────────────────────────────────────┘
```

### Interaction Model

1. **SAT solver** assigns Boolean values to atoms (ground theory predicates)
   - Example: Set `(x < 5)` to true, `(f(x) = a)` to true, `(a ≠ b)` to false
2. **Theory solver** checks: "Is this assignment T-satisfiable?"
   - Asks: Is there an interpretation where `x < 5`, `f(x) = a`, and `a = b` all hold?
3. **If SAT**: Continue searching for a complete assignment
4. **If UNSAT**: Theory solver returns a conflicting subset of literals
   - SAT solver learns a new clause preventing this conflict
   - Backtracks and explores a different assignment

### Theory Propagation

The theory solver can also **propagate** consequences:

```
Currently assigned: x < 5, f(x) = a, f(y) = b, y ≥ 5

Theory solver notices: x < 5 and y ≥ 5 → x ≠ y
                       f is a function → f(x) ≠ f(y)
                       Therefore: a ≠ b

Consequence: Theory solver forces the literal (a ≠ b) to true
SAT solver uses this as a new unit clause for propagation
```

This **theory-driven propagation** can dramatically prune the search space.

### Incremental Solving

Modern DPLL(T) solvers support **incremental interface**:
```python
solver.push()           # Save current state
solver.add(constraint)  # Add new constraint
result = solver.check() # Re-check satisfiability
solver.pop()            # Revert to saved state
```

The theory solver caches partial results and backtracks efficiently, making interactive query workflows practical.

---

## Part IV: How Z3 Combines Theories

Z3 is a production SMT solver developed by Microsoft Research. Its approach to theory combination has evolved from pure Nelson-Oppen to **model-based theory combination**, a key innovation that makes Z3 significantly faster in practice.

### Z3's Theory Architecture

Z3 organizes solvers as a **plugin system**:

```
┌─────────────────────────────────────────────┐
│  SAT/CDCL Core (Conflict-Driven Clause Ld.) │
└────────────────┬────────────────────────────┘
                 │
      ┌──────────┼──────────┐
      ↓          ↓          ↓
  ┌────────┐ ┌────────┐ ┌────────┐
  │Arith.  │ │ EUF    │ │ Arrays │  ... (pluggable theories)
  │Solver  │ │Solver  │ │Solver  │
  └────────┘ └────────┘ └────────┘
```

Each theory solver:
- Receives assertions (conjunctions of theory literals)
- Checks satisfiability and reports conflicts
- Propagates consequences and learns lemmas
- Communicates via shared variables

### Model-Based Theory Combination (The Z3 Innovation)

Traditional Nelson-Oppen requires theories to report **all implied equalities**. This is expensive: computing all consequences can require exhaustive case analysis.

Z3's approach: **Only reconcile equalities that appear in the candidate model.**

**Algorithm**:
1. SAT solver finds a partial assignment
2. Each theory solver builds a **candidate model** consistent with its constraints
3. Check: Do all theory models agree on equalities of shared variables?
   - For each shared variable `v`: If `Theory_A` assigns `v = val_A` and `Theory_B` assigns `v = val_B`:
     - If `val_A = val_B`, consistency is satisfied
     - If `val_A ≠ val_B`, add the constraint `v = val_B` to Theory_A (or vice versa) to reconcile
4. **Reconciliation**: Add equalities to any theory that needs them until models align
5. If reconciliation succeeds, models agree on all shared symbols → **SAT**
6. If reconciliation fails (contradiction), learn conflict clause and backtrack

**Benefit**: We only compute equalities actually needed for the current model, not all possible equalities. This is much more efficient when the theory solvers have wide latitude in their models.

### Example: Z3 Combining Arithmetic and EUF

**Formula**:
```
(x < 5) ∧ (f(x) = a) ∧ (f(5) = b) ∧ (a = b)
```

**Step 1**: SAT solver assigns all atoms to true:
- `(x < 5)` := true
- `(f(x) = a)` := true
- `(f(5) = b)` := true
- `(a = b)` := true

**Step 2**: Arithmetic solver builds model:
```
x = 2    (satisfies x < 5)
```

**Step 3**: EUF solver tries to build model:
```
f is a function
f(2) = a  (from f(x) = a with x = 2)
f(5) = b  (from f(5) = b)
a = b     (asserted)

But wait: if a = b and f(2) = a and f(5) = b, 
then f(2) = f(5), meaning f(2) = f(5).
However, 2 ≠ 5, so f is not injective. That's OK.

Model: f(2) = f(5) = c (some value), a = b = c
```

**Step 4**: Reconcile models:
- Arithmetic has: x = 2
- EUF has: x appears in f(x) = a; in EUF's model, x = 2 (shared interpretation)
- Both models agree ✓

**Result**: **SAT** with model `x = 2, f(2) = f(5) = c, a = b = c`

### Z3's Propagation Mechanism

Beyond reconciliation, Z3 uses **lemmas** to propagate theory consequences:

```
Theory solver discovers: 
  From (x < 5) and (x > 0) and EUF constraints,
  we can deduce (f(x) ≠ 0)

Theory creates a lemma:
  (x < 5) ∧ (x > 0) → (f(x) ≠ 0)
  
Encoded as a clause for SAT solver:
  ¬(x < 5) ∨ ¬(x > 0) ∨ (f(x) ≠ 0)
```

These **theory-learned lemmas** restrict the search space and accelerate solving.

---

## Part V: Convexity and Non-Convexity in Theory Combination

Not all theory combinations are equally well-behaved. The property of **convexity** determines whether Nelson-Oppen's propagation-only protocol is sufficient, or whether case splitting is necessary.

### Definitions

**A theory T is convex if**: Whenever T entails a disjunction of equalities `(x = a) ∨ (x = b) ∨ ... ∨ (x = z)`, it entails at least one of the disjuncts.

**Equivalently**: The set of models satisfying a conjunction of T-literals is convex in variable assignments.

### Examples

#### Convex Theories

**Linear Arithmetic over Reals (LA(ℝ))**
```
Constraints: x < 5, x > 2, y = x + 1

Models satisfy: 2 < x < 5, 3 < y < 6

If a model has x = 3 and another has x = 4, 
then all intermediate values are also models.
Convex ✓
```

**Equality and Uninterpreted Functions (EUF)**
```
Constraints: f(a) = b, f(c) = d, a = c

Models have: a = c (no choice)

EUF is convex ✓
```

#### Non-Convex Theories

**Linear Arithmetic over Integers (LA(ℤ))**
```
Constraints: x < 5, x > 2, x ∈ ℤ

Models: x ∈ {3, 4}

If we ask "must x = 3.5 or x = 4?", 
LA(ℤ) entails "x = 3 ∨ x = 4" but not a single disjunct.
Non-convex ✗
```

### Impact on Combination

#### Combining Two Convex Theories

If both T1 and T2 are convex and both have polynomial decision procedures, then **Nelson-Oppen's equality propagation alone** suffices:

```
Time complexity: O(P1 × P2 × |shared_vars|)
where P1, P2 are the complexity of individual solvers
```

No case splitting is needed; pure propagation finds the answer.

#### Combining with a Non-Convex Theory

If one theory is non-convex (like integer arithmetic), the propagation protocol must be augmented with **case analysis**:

```
Formula: (x < 5) ∧ (x > 2) ∧ f(x) = a ∧ f(4) = b ∧ (a ≠ b)

Arithmetic forces: x ∈ {3, 4}  (disjunctive)

SAT solver must try both cases:
  Case 1: Assume x = 3 → Check EUF satisfiability
  Case 2: Assume x = 4 → Check EUF satisfiability
```

This is why **combining integer arithmetic with other theories** is harder than combining reals: integer constraints naturally lead to disjunctions that require search.

### Combining Two Non-Convex Theories

Both must have nondeterministic polynomial procedures (NP). Case analysis may be exponential.

### Convexity in Z3

Z3 tracks convexity as a theory property and uses it to optimize:
- **Convex theories** → Pure propagation
- **Non-convex theories** → SAT solver drives case analysis

---

## Part VI: Online vs. Offline Combination

Two strategies for arranging theory solvers:

### Offline Combination

Theories are combined **before solving starts**.

**Approach**:
1. Declare all theories upfront (arithmetic, EUF, arrays, etc.)
2. Purify the input formula
3. Build a combined solver
4. Solve

**Advantage**: Solver can be fully optimized for the specific theory combination.

**Disadvantage**: Adding new theories or changing the theory combination requires rebuilding the solver.

**Used by**: Most production SMT solvers (Z3, CVC5) operate this way internally, though they support dynamic theory addition.

### Online Combination

Theories are combined **dynamically during solving**.

**Approach**:
1. Start with a minimal solver
2. As formulas are added, detect which theories are needed
3. Instantiate those theory solvers on-the-fly
4. Coordinate them during solving

**Advantage**: Can handle unknown or dynamic theory combinations; minimal overhead for problems using few theories.

**Disadvantage**: Slower theory orchestration; less optimization opportunity.

**Used by**: Some specialized or teaching-oriented SMT solvers; systems that adapt to user needs.

### Hybrid Approach

Many modern solvers use a **hybrid**:
- Fixed set of common theories (arithmetic, EUF, arrays) are always available
- Less common theories (strings, datatypes) are loaded on demand
- At query time, only active theory solvers participate

---

## Part VII: Extensions Beyond Classical Nelson-Oppen

Classical Nelson-Oppen requires **stably infinite + disjoint signatures**. Real SMT solvers handle cases that violate these assumptions.

### Combining Non-Stably Infinite Theories

**Problem**: Bit-vectors have finite domains (2^32 or 2^64 values). Combining bit-vectors with a stably infinite theory (e.g., reals) violates the stably infiniteness requirement.

**Solution 1: Shininess (Tinelli & Zarba, 2003)**

A stronger property than stably infiniteness:

**Definition**: A theory T is **shiny** if every partial model can be extended to a model that respects the shared variables.

**Consequence**: Shiny theories can be combined with any decidable theory, even non-stably infinite ones.

**Example**: Bit-vectors are shiny (you can always assign values to any variables), but the integers are not (some formulas force x = 5.5, which has no integer model).

**Solution 2: Separate Domains**

Keep finite and infinite domains segregated:
- Bit-vectors operate on a separate domain (finitary)
- Reals and strings operate on an infinite domain
- Interactions between domains use explicit conversion functions

### Overlapping Signatures

When theories share function symbols (e.g., both define `+`), we need **shared function semantics**.

**Approach**: Define a unified semantics for shared symbols.

**Example**: Both arithmetic and bit-vectors have addition. The solver uses:
- Arithmetic semantics for unbounded operations
- Bit-vector semantics for fixed-width operations
- Conversion functions (int_to_bv, bv_to_int) at boundaries

This is not pure Nelson-Oppen (which requires disjoint signatures) but a careful merging of theories with disciplined boundaries.

---

## Part VIII: Relating to Evident — Language-Level Combination

Now, the key connection to Evident:

### The Analogy

In Z3 and DPLL(T):
- **Theory solvers** receive conjunctions of literals
- **Solvers communicate** via shared variables and equalities
- **Propagation** drives inference across theory boundaries
- **Case splitting** (via DPLL) handles non-convexity
- **Backtracking** reverts to explore alternatives

In Evident:
- **Schemas** are constraint sets (conjunction of membership conditions)
- **Schemas can reference shared variables** (composed schemas)
- **A schema query asks**: "Do satisfying assignments exist?"
- **Schema composition** creates dependencies (e.g., `task ∈ Task` expands task fields in parent scope)

### The Vision: Schemas as Cooperative Solvers

Imagine an Evident program with multiple schemas:

```
type Task = {
  id: Int,
  duration: Int,
  assigned_to: Person
}

type Person = {
  id: Int,
  max_load: Int
}

schema ValidAssignment(t: Task, p: Person):
  t.assigned_to = p.id
  p.max_load ≥ t.duration

schema ConsistentTeam(people: [Person], tasks: [Task]):
  ∀ p ∈ people: p.id ∈ {1..10}
  ∀ t ∈ tasks: 
    ∃ p ∈ people: ValidAssignment(t, p)
```

Here:
- `ValidAssignment` is a "theory solver" for task-person assignments
- `ConsistentTeam` is a higher-level "theory" that coordinates multiple assignments
- The language automatically:
  1. Detects shared variables (ids, durations)
  2. Propagates equalities across schema boundaries
  3. Performs case analysis when needed (non-convex constraints)
  4. Finds a joint model satisfying all schemas

### Soundness and Completeness via Nelson-Oppen

For this to work correctly, we need:

**Requirement 1: Disjoint Variable Scopes**
Schemas should have non-overlapping variable names unless explicitly composed. This mirrors the "disjoint signatures" requirement—variables play the role of function symbols.

**Requirement 2: Stably Infinite Constraints**
Each schema's constraints should admit infinite models (or we use shinier theories). If we allow finite datatypes (like enums), we need careful handling.

**Requirement 3: Equality Propagation**
When a schema deduces `x = 5`, this equality should propagate to other schemas referencing `x`. This is the "equality sharing protocol" at the language level.

**Theorem (Nelson-Oppen applied to Evident)**:
If schemas have disjoint variable domains and stably infinite constraints, the Evident runtime can:
1. Purify composed schemas (expand sub-schemas)
2. Create individual Z3 solvers for each schema (or unified solver with theory partitioning)
3. Propagate equalities bidirectionally
4. Find a joint satisfying assignment iff one exists

### When Schemas Violate Soundness Conditions

**Case 1: Non-disjoint variable scopes**
```
schema A(x: Int): x > 5
schema B(x: Int): x < 10
```

Both define `x`. If they're queried independently, there's no issue. If composed (implicitly sharing `x`), then Evident must reconcile the scopes. This is safe if we explicitly use schema composition syntax.

**Case 2: Finite domain constraints**
```
type Color = Red | Green | Blue
schema Paint(c: Color): c ≠ Red
```

The domain of `c` is finite (3 values). If composed with an arithmetic schema expecting infinite integers, we need "shiny" reasoning or domain separation.

**Case 3: Non-convex constraints at language level**
```
schema OptionalDependency(task: Task, other: Task):
  (task.depends_on = null) ∨ (task.depends_on = other.id)
```

The disjunction at the schema level is non-convex. Evident's runtime will rely on Z3's case splitting (DPLL(T)) to handle it, which is correct.

---

## Part IX: Concrete Examples of Theory Combination

### Example 1: Arithmetic + Uninterpreted Functions

**Problem**:
```
x < 5
y = f(x)
f(3) = 10
y ≤ 20
```

**Theories**:
- **Arithmetic**: `x < 5, y ≤ 20`
- **EUF**: `y = f(x), f(3) = 10`
- **Shared variables**: `x, y, f` (via function application)

**Propagation**:
1. Arithmetic deduces: `x ∈ [min_int, 4]`
2. EUF asks: Given `x ∈ [min_int, 4]`, can `y = f(x), f(3) = 10, y ≤ 20`?
3. EUF deduces: If `x = 3`, then `y = f(3) = 10` (satisfies `y ≤ 20`). Otherwise, `f(x)` can be anything ≤ 20.
4. No contradiction. **SAT** with model: `x = 0, y = f(0) = 5` (for example)

### Example 2: Integer Arithmetic + Arrays

**Problem**:
```
a[i] = 5
i < 10
a[10] = 3
a[i] ≠ a[10]
```

**Theories**:
- **Integer Arithmetic**: `i < 10, i ≥ 0`
- **Array Theory**: `a[i] = 5, a[10] = 3, a[i] ≠ a[10]`
- **Shared variables**: `i, a` (array and index)

**Propagation**:
1. Arithmetic deduces: `i ∈ [0, 9]`
2. Array theory checks: If `i ∈ [0, 9]`, is `a[i] = 5, a[10] = 3, a[i] ≠ a[10]` consistent?
3. Since `i ≠ 10` (i ∈ [0, 9]), array theory allows independent values: `a[i] = 5` and `a[10] = 3`. ✓
4. Array theory deduces: `i ≠ 10` (forced by arithmetic)
5. **SAT** with model: `i = 5, a[5] = 5, a[10] = 3`

### Example 3: Arithmetic + Arithmetic (Non-Convex)

**Problem**:
```
(x < 10) ∧ (x > 0) ∧ (x ∈ ℤ)
f(x) = x^2
f(x) > 50
f(x) < 100
```

**Theories**:
- **Linear Arithmetic (reals)**: `y = x^2, y > 50, y < 100`
- **Integer Arithmetic**: `0 < x < 10, x ∈ ℤ`
- **Shared variable**: `x, y`

**Propagation**:
1. Linear AR deduces: `x ∈ (-∞, -√50) ∪ (√50, ∞)` and `x ∈ (-10, 10)`
   - Intersection: `x ∈ (-10, -√50) ∪ (√50, 10)` where `√50 ≈ 7.07`
2. Integer AR intersects: `x ∈ {8, 9}` (the integers in (√50, 10))
3. Case 1: `x = 8`, then `y = 64` ∈ (50, 100) ✓
4. Case 2: `x = 9`, then `y = 81` ∈ (50, 100) ✓
5. **SAT** with models: `(x=8, y=64)` or `(x=9, y=81)`

The integer constraint's non-convexity (forcing discrete choices) is handled by SAT-level case splitting.

### Example 4: Evident Schemas as Theories

**Problem**: Scheduling with resource constraints

```
type Task = {
  id: Int,
  duration: Int,
  resource: String
}

type Resource = {
  name: String,
  capacity: Int
}

schema TaskValid(t: Task):
  t.duration > 0
  t.duration < 1000

schema ResourceCapacity(r: Resource):
  r.capacity > 0
  r.capacity ≤ 100

schema Allocation(tasks: [Task], resources: [Resource]):
  ∀ t ∈ tasks: TaskValid(t)
  ∀ r ∈ resources: ResourceCapacity(r)
  ∀ t ∈ tasks: ∃ r ∈ resources: t.resource = r.name
```

**Nelson-Oppen Analysis**:
- Each schema is a "theory"
- Shared variables: `tasks, resources` (via reference)
- Disjoint variable scopes: Each schema's internal variables (e.g., loop indices) are local
- Equality propagation: If one schema deduces `t.resource = "CPU"`, this propagates to all schemas referencing `t`

**Runtime**:
1. Expand `Allocation` into a Z3 context
2. Instantiate Z3 constants for each task and resource
3. Assert constraints from each schema
4. Solver finds an allocation satisfying all schemas simultaneously

**Soundness**: By Nelson-Oppen, if schemas are stably infinite and have non-overlapping core constraints (only coordinate via equalities), the combined query is sound and complete.

---

## Part X: Implementation Lessons for Evident

### Lesson 1: Variable Purification

When composing schemas, Evident's runtime should:
1. Detect shared variables (by name and type)
2. Introduce proxy variables for composed sub-schemas
3. Add equality constraints linking proxies to originals

**Example**:
```
task ∈ Task  // introduces task.id, task.duration, task.resource

In Z3:
  task__id : Int
  task__duration : Int
  task__resource : String
```

### Lesson 2: Equality Propagation

Z3 handles equality propagation automatically via its EUF solver. Evident benefits because:
- `x = 5` in arithmetic propagates to EUF constraints on `x`
- Equality atoms learned by SAT solver are available to all theories

### Lesson 3: Theory Convexity

Evident should allow schemas to declare convexity properties:

```
@convex
schema LinearScheduling(...):
  // Only linear arithmetic, EUF
  // No case splitting needed for this schema alone
```

This helps the runtime decide whether pure propagation or DPLL(T) case splitting is needed.

### Lesson 4: Scoping and Composition

Clear scoping rules prevent variable collision:
- Schema-local variables are fresh (e.g., loop indices in `∀ t ∈ tasks`)
- Composed schemas reference parent variables by dotted names
- Explicit bindings link parameters to arguments

**Example**:
```
// Local variable: 'i'
∀ i ∈ Nat, i < 10: (task[i].duration > 0)

// Shared variable: 'task'
task ∈ Task
task.duration = 5  // 'task' is shared with parent query
```

### Lesson 5: Conflict Explanation

When Z3 reports unsatisfiability, Evident can trace back:
1. Which schemas contributed to the conflict
2. Which equality propagation led to it
3. Provide user-friendly feedback

**Example**:
```
UNSAT because:
  TaskValid(t1) requires t1.duration > 0
  But Allocation deduces t1.duration = 0 from ResourceCapacity(r1)
```

---

## Part XI: Open Questions for Evident

1. **Schema-level non-convexity**: When a schema has disjunctive constraints, how should the runtime handle them? Should schemas explicitly declare case-splitting regions?

2. **Partial models**: Can Evident support querying a schema for a "partial satisfying assignment" (some variables unbound)? This would align with how theory solvers work incrementally.

3. **Theory composition with overlapping domains**: If two schemas both reason about integers, should Evident treat them as separate theories (via variable renaming) or as a single theory? Trade-off between modularity and efficiency.

4. **Incremental schema refinement**: Can Evident support adding constraints to a schema incrementally without rebuilding the entire Z3 context? This mirrors Z3's `push()`/`pop()` interface.

5. **Performance profiling**: Which schemas are bottlenecks? How much time is spent in equational reasoning vs. arithmetic? Evident could profile theory solver performance and suggest optimizations.

---

## Conclusion

The Nelson-Oppen combination framework reveals a deep truth: **the cooperative solving of multiple constraints via equality sharing is universal**. Z3's internal architecture uses it; Evident can expose the same pattern at the language level.

Key takeaways:

1. **Purification + Propagation**: Separate concerns (arithmetic, functions) and unite them through shared variables.

2. **Stably Infinite & Disjoint**: These are the soundness requirements. Violations require special handling (shininess, domain separation).

3. **Convexity Matters**: Affects whether pure propagation suffices or case splitting is needed.

4. **DPLL(T)** orchestrates multiple theories via a SAT solver, enabling full reasoning over complex formulas.

5. **Z3's Model-Based Approach**: Only reconcile equalities appearing in candidate models, not all possible equalities. Efficient in practice.

6. **Evident Parallel**: Schemas are mini-theories cooperating through composition and equality sharing. Nelson-Oppen guarantees correctness if these patterns are followed.

---

## References and Further Reading

- [Nelson-Oppen Theory Combination (Stanford Lecture)](https://web.stanford.edu/class/cs357/lecture11.pdf)
- [DPLL(T) Paper (Nieuwenhuis, Oliveras, Tinelli)](https://homepage.cs.uiowa.edu/~tinelli/papers/NieOT-JACM-06.pdf)
- [Z3: An Efficient SMT Solver](https://link.springer.com/chapter/10.1007/978-3-540-78800-3_24)
- [Programming Z3 (de Moura & Bjørner)](https://theory.stanford.edu/~nikolaj/programmingz3.html)
- [Combining Non-Stably Infinite Theories](https://homepage.cs.uiowa.edu/~tinelli/papers/TinZar-RR-03.pdf)
- [SMTMSMT: Gluing Together CVC5 and Z3 Nelson Oppen Style](https://www.philipzucker.com/glue-cvc5-z3/)
- [Combining Combination Properties (2025)](https://link.springer.com/article/10.1007/s10817-025-09746-5)
- [Satisfiability Modulo Theories (Berkeley SMT Book Chapter)](https://people.eecs.berkeley.edu/~sseshia/pubdir/SMT-BookChapter.pdf)
