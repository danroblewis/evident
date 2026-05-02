# Constraint Handling Rules (CHR): Research & Design Document

**Author:** Research compilation for Evident language design  
**Date:** 2026-04-30  
**Status:** Design reference for constraint system architecture

## Executive Summary

Constraint Handling Rules (CHR) is a declarative, rule-based language for writing constraint solvers as forward-chaining rule systems. Introduced in 1991 by Thom Frühwirth, CHR executes as a committed-choice multiset rewriting engine that operates on a constraint store, applying rules that simplify, propagate, or eliminate constraints until a stable state is reached.

This document explores CHR's formal model, execution semantics, and potential applications to Evident—a Z3-backed constraint language where multiple schema-based solvers could cooperate through inter-schema rule firing, similar to how CHR's multi-headed rules enable constraint networks.

---

## 1. What is Constraint Handling Rules?

### 1.1 Origins and Philosophy

CHR is a **domain-specific language for implementing constraint solvers**. Rather than embedding constraints in a host language (like Prolog or functional languages), CHR treats constraint programming as a first-class citizen with:

- **Declarative syntax**: Rules describe *what* constraints mean, not *how* to solve them
- **Operational semantics**: Clear execution model based on pattern matching and rule firing
- **Host language embedding**: CHR compiles to a host language (Prolog, Haskell, Java, etc.)
- **Logic foundation**: Semantics grounded in classical and linear logic

Key insight: **CHR separates the "what" (declarative meaning) from the "how" (operational execution).** A CHR program can be understood logically (what constraints must hold) independently of when rules fire (execution order).

### 1.2 Core Abstraction: The Constraint Store

The constraint store is a **multiset of constraints** (facts) that evolves as rules fire:

```
Initial state:  {constraint₁, constraint₂, constraint₃, ...}
                    ↓ (rule A fires)
After rule A:   {constraint₂, constraint₃', constraint₄, ...}
                    ↓ (rule B fires)
Final state:    {constraint₂, constraint₃', constraint₄', constraint₅, ...}
```

A constraint is simply a predicate applied to terms: `leq(X, Y)`, `type(var42, Int)`, `task_duration(task1, 8)`.

Rules pattern-match against constraints in the store and fire when:
1. All head constraints exist in the store
2. All guard conditions are satisfied
3. The rule has not been applied with that exact combination before (for propagation rules)

---

## 2. The Formal Model

### 2.1 Syntax

A CHR program consists of **named rules** with three syntactic forms:

#### Simplification Rules
```
head1, head2 <=> guard | body.
```
**Semantics**: When `head1` and `head2` match, *replace* them with `body` (if guard holds).

**Example** (less-or-equal constraint solver):
```
% Reflexivity: X ≤ X is always true
leq(X, X) <=> true.

% Antisymmetry: if X ≤ Y and Y ≤ X, then X = Y
leq(X, Y), leq(Y, X) <=> X = Y.
```

#### Propagation Rules
```
head1, head2 ==> guard | body.
```
**Semantics**: When `head1` and `head2` match, *keep* them and *add* `body` (if guard holds).

**Example** (continue from above):
```
% Transitivity: if X ≤ Y and Y ≤ Z, then X ≤ Z
leq(X, Y), leq(Y, Z) ==> leq(X, Z).
```

#### Simpagation Rules
```
head1 \ head2 <=> guard | body.
```
**Semantics**: When `head1` and `head2` match, *keep* `head1`, *replace* `head2` with `body`.

**Example**:
```
% Keep the domain constraint, replace it with narrower one if possible
domain(X, Dom1) \ domain(X, Dom2) <=> 
    NewDom = Dom1 ∩ Dom2 | 
    domain(X, NewDom).
```

### 2.2 Guard Conditions

Guards are **passive constraints** (built-in predicates) that test applicability without modifying the store or binding head variables.

```
simplification_example(X, Y) <=> 
    X > 0, Y > 0 |  % These are the guards
    sum(X, Y).
```

Guard rules:
- May test arithmetic (`X > 0`, `X = 5`)
- May test structural properties (`var(X)`, `atom(X)`)
- **Cannot** bind variables that appear in the head
- **Can** introduce fresh variables for use in the body

### 2.3 The Three Semantics Levels

| Level | Purpose | Usage |
|-------|---------|-------|
| **Declarative** | What constraints logically mean | Proof/verification |
| **Operational** | How rules fire sequentially | Implementation/debugging |
| **Pragmatic** | Efficiency & propagation history | Actual execution |

For a terminating, confluent program, all three agree on final answers.

---

## 3. Execution Algorithm & Multi-Headed Rules

### 3.1 The Core Loop

CHR execution follows a **forward-chaining** pattern:

```
Input: Constraint store C (initially empty)
       Query Q (constraints to satisfy)

Loop:
  1. Add constraints from Q to store C
  2. For each applicable rule R:
       a. Find all combinations of head constraints matching R
       b. Check if guard succeeds
       c. Check if R hasn't fired with this combination before (for propagation)
       d. Fire: remove/replace heads, add body to store
  3. If new constraints added, go to step 2; else DONE
```

### 3.2 Multi-Headed Rule Matching

The key innovation of CHR is **multi-headed rules**: a single rule can match *multiple* constraints simultaneously.

Example with a 3-headed rule:

```
% Two tasks can be scheduled sequentially if their duration fits in a day
task(T1, Dur1), task(T2, Dur2), capacity(Day, Cap) <=> 
    Dur1 + Dur2 <= Cap | 
    scheduled(T1, Day), scheduled(T2, Day).
```

When matching, the CHR engine must:
- Locate `task(T1, Dur1)` in the store
- Locate `task(T2, Dur2)` with `T1 ≠ T2`
- Locate `capacity(Day, Cap)` with matching `Day`
- Test the guard `Dur1 + Dur2 <= Cap`
- If all match, fire once

This is fundamentally different from a Prolog-style conjunction, because:
- The constraints are *consumed* from the store (simplification) or *kept* (propagation)
- Pattern matching is against the store, not proof search
- Rule firing is **committed** (no backtracking to try other rule combinations)

### 3.3 Propagation History

For **propagation rules**, the engine maintains a **propagation history** (or **occurrence counter**) to ensure each rule fires at most once with a given set of head constraints.

```
Store: {leq(X, Y), leq(Y, Z)}

Rule: leq(X, Y), leq(Y, Z) ==> leq(X, Z).

After firing:
Store: {leq(X, Y), leq(Y, Z), leq(X, Z)}
Propagation history: {(leq,leq,→): [(X,Y), (Y,Z)]}

If store later receives new leq constraints, rule may fire again with different combinations.
```

This is essential to prevent infinite looping with propagation rules.

### 3.4 Rule Application Order

The CHR execution model is **order-independent** (confluent programs): different orderings of rule application yield the same final constraint store.

However, actual implementations use heuristics:
- **Lazy matching** (LEAPS algorithm): Only re-check rules when new constraints are added
- **Occurrence-based ordering**: Track which constraints trigger which rules
- **Priority-based firing**: Apply higher-priority rules first

---

## 4. Confluence and Termination Properties

### 4.1 Confluence

A CHR program is **confluent** if all possible execution orders lead to the same final constraint store.

**Critical pairs** are the source of non-confluence. If two rules can fire on overlapping constraint combinations and produce different results, they form a critical pair:

```
Example of non-confluent rules:

% Rule 1: Simplify constraint A
a(X) <=> b(X).

% Rule 2: Also matches A and produces different result
a(X) <=> c(X).

Query: a(1)

Execution 1 → Store: {b(1)}
Execution 2 → Store: {c(1)}
Result: NON-CONFLUENT
```

**Confluence check methods**:
1. **Manual proof**: Show all critical pairs are resolvable
2. **Automatic analysis**: Use tools like CHRisTA or AProVE
3. **Design discipline**: Avoid overlapping rule heads

### 4.2 Termination

A CHR program **terminates** if all possible computations are finite (no infinite loops of rule firing).

Challenges:
- **Propagation rules can loop**: If rules keep adding constraints that trigger other rules
- **Simplification rules are safer**: Since they consume constraints, the store size decreases

**Termination proof techniques**:

1. **Decrease measure**: Show some metric decreases with each rule firing
   - Store size (for simplification)
   - Lexicographic order on constraint arguments

2. **Termination by approximation**: Prove ground-termination (concrete inputs) via CLP analysis

3. **Levels and strata**: Organize rules so that higher-level rules don't trigger lower ones

**Example**: Unification solver terminates because variable bindings decrease, and once a variable is bound, that simplification never fires again.

### 4.3 Combined Properties

**Theorem** (Newman, 1942): If a program is **locally confluent** and **terminating**, then it is **confluent**.

Local confluence: any two rule firings on the same constraint combination can be resolved (produce the same subsequent state).

For well-designed CHR programs:
- **Termination + confluence = canonical answers**: Every query has exactly one stable result
- This is why CHR works for **type inference** (guaranteed unique types) and **scheduling** (guaranteed optimal layouts)

---

## 5. Comparing CHR to Standard Constraint Programming

| Aspect | CHR | Standard CP (e.g., Z3) |
|--------|-----|----------------------|
| **Model** | Multiset rewriting of constraints | Constraint store + solver theory |
| **Rule firing** | Forward-chaining, committed choice | Solver decides propagation internally |
| **User control** | Write domain-specific rules | Query a monolithic solver |
| **Declarative semantics** | Logical equivalence (can read as Horn clauses) | SMT theory semantics |
| **Termination** | Must be proven/designed by user | Solver always terminates |
| **Confluence** | Must be verified | Not applicable (solver decides) |
| **Parallel execution** | Natural (rules on different constraints) | Limited (solver internals not exposed) |
| **Interoperability** | Multiple independent constraint stores | Single monolithic store |

### Key Insight for Evident

**CHR is essentially a *meta-language* for writing constraint solvers**, while Z3 is a *solver*. Evident's approach of using Z3 as the backend is orthogonal to adopting CHR-like inter-schema communication:

- Z3 solves *individual schema queries* (given constraints, find a satisfying model)
- CHR-style rules could *coordinate* between schemas (when Schema A derives a fact, trigger Schema B's queries)

Example (pseudo-Evident with CHR-inspired rules):

```
schema Person {
  name ∈ String
  age ∈ Nat
  role ∈ {Employee, Manager}
}

schema Org {
  manager_id ∈ Nat
  team_budget ∈ Nat
}

% CHR-inspired rule: when a person is appointed manager, 
% allocate a budget and trigger Org schema
when person ∈ Person, person.role = Manager
  → org ∈ Org, org.manager_id = person.id
      [allocate_budget(org, 50000)]
```

---

## 6. Real-World CHR Applications

### 6.1 Type Inference

CHR excels at type inference because:
- **Unification constraints** are natural multi-headed simplifications
- **Confluence ensures correctness**: If type system rules are confluent, inferred type is unique
- **No backtracking needed**: Unlike Hindley-Milner search

**Example** (simplified Hindley-Milner in CHR):

```
% Constraint: variable X has type T1, but also type T2
% Simplify: unify T1 and T2, keep one
type(X, T1), type(X, T2) <=> T1 = T2 | type(X, T1).

% Propagate: if function f maps A→B and we apply f to type A, infer output is B
func_type(F, arrow(A, B)), app_type(F, A) ==> app_type(F, B).

% Simplify: redundant application constraints
app_type(F, T), app_type(F, T) <=> app_type(F, T).
```

Results:
- Type inference is **decidable** and **complete** (if it terminates)
- The inferred type is independent of rule ordering (confluence)

### 6.2 Scheduling Problems

Scheduling naturally decomposes into constraints and propagation:

```
% Constraints: tasks, durations, precedences
task(T1, Duration1), task(T2, Duration2), 
  ordered_before(T1, T2)  <=> 
  % Propagate: T1 must finish before T2 starts
  start(T1, S1), start(T2, S2) ==>
  S1 + Duration1 <= S2.

% Redundancy elimination: if we've already inferred a precedence, remove duplicate
precedence(T1, T2), precedence(T1, T2) <=> precedence(T1, T2).

% Load balancing: if a worker is overloaded, redistribute tasks
worker(W, Load), Load > MaxCapacity, task(T, Dur, W) <=>
  % Simplify: reassign task to less loaded worker
  find_available_worker(W2),
  task(T, Dur, W2), worker(W, Load - Dur), worker(W2, Load2 + Dur).
```

**Advantages**:
- Incremental: add constraints and propagation happens automatically
- Modular: each domain (precedence, capacity, load-balancing) is a separate rule set
- Analyzable: can verify termination and optimality separately

### 6.3 Other Applications

Per research:
- **Static analysis**: Type checking for incomplete programs
- **Grammar induction**: Parsing and language processing
- **Natural language processing**: Constraint-based grammar rules
- **Symbolic constraint solving**: Linear algebra, polynomial systems
- **Graph coloring**: Map coloring (4-color theorem applications)

---

## 7. CHR's Model of Cooperating Solvers

### 7.1 The Core Idea: Decentralized Constraint Networks

Traditional constraint systems have a single monolithic store and one solver. CHR enables **multiple independent constraint stores** that communicate through **rule firing across store boundaries**.

Formal model (from "Decentralized Execution of Constraint Handling Rules for Ensembles"):

```
Ensemble = {Store₁, Store₂, Store₃, ...}

Each store maintains its own set of constraints.
Rules can:
  - Match constraints within a store (intra-store rules)
  - Match constraints across stores (inter-store rules)

When a constraint is added to any store, all applicable rules fire,
potentially triggering constraint additions in neighboring stores.
```

### 7.2 Relevance to Evident's Schema Architecture

Evident's design uses **schemas** as named constraint systems. Each schema query:
1. Instantiates variables
2. Translates constraints to Z3
3. Solves via Z3
4. Returns a model (evidence of satisfiability)

**CHR-inspired extension**: Multi-schema communication via rules that trigger across schemas.

```
Current Evident:
  schema Person { ... }     →  Z3 solver for Person constraints
  schema Company { ... }    →  Z3 solver for Company constraints
  (Schemas are independent)

CHR-inspired Evident:
  schema Person { ... }
  schema Company { ... }
  
  % Rule: if person P is a manager, ensure company has a manager
  when (p ∈ Person, p.role = Manager)
    → (c ∈ Company, c.has_manager = true)
        [check that p.id ∈ c.employee_ids]
```

This would require:
- **Query composition**: Person.query(p.role = Manager) triggers Company.query(p.id ∈ ...) 
- **Constraint propagation**: Results from one schema constrain the other
- **Fixpoint computation**: Rules fire until no new constraints can be derived

### 7.3 Distributed & Decentralized Execution

CHRe (Constraint Handling Rules for Ensembles) extends CHR to distributed systems:

Each computing entity has:
- Local constraint store
- Local rule set
- Communication channels to neighbors

Rules can:
- Read from own store and neighbors' stores
- Write to own store and neighbors' stores

Application to Evident:
```
% In a multi-node query federation
schema Person@Node1 { ... }
schema Person@Node2 { ... }
schema SyncRule {
  % Sync person data across nodes
  person(ID, Name)@Node1, person(ID, Name)@Node2 <=> true.
  
  % Propagate constraints
  person(ID, Name)@Node1 ==> person(ID, Name)@Node2.
}
```

Benefits:
- **Horizontal scalability**: Many schemas on many nodes
- **Fault tolerance**: Schemas can be replicated
- **Local autonomy**: Each schema has independent solver instance

---

## 8. CHR Implementations

### 8.1 Production Systems

#### SWI-Prolog CHR
- **Status**: Stable, integrated into SWI-Prolog 7+
- **Implementation**: Compilation to Prolog + runtime library
- **Features**: All standard CHR operators, well-documented
- **URL**: https://www.swi-prolog.org/pldoc/man?section=chr

```prolog
:- use_module(library(chr)).

:- chr_constraint leq/2.

leq(X, X) <=> true.
leq(X, Y), leq(Y, X) <=> X = Y.
leq(X, Y), leq(Y, Z) ==> leq(X, Z).
```

#### SICStus Prolog CHR
- **Status**: Original CHR implementation (Frühwirth's system)
- **Implementation**: C library compiled from CHR rules
- **Maturity**: Well-tested, used in commercial applications
- **URL**: https://sicstus.sics.se/sicstus/docs/latest/html/sicstus/

### 8.2 Research & Modern Implementations

#### Haskell CHR (HCHR)
- Embedded in Haskell via library
- Type-safe constraint definitions
- Suitable for functional programming style

#### FreeCHR
- **Status**: Algebraic framework for CHR embeddings
- **Focus**: Formalize the embedding of CHR into host languages
- **Papers**: Recent work on operational semantics and optimization

#### CHRe (Ensembles)
- **Focus**: Decentralized, distributed execution
- **Applications**: Multi-agent systems, sensor networks
- **Research area**: Still primarily academic

### 8.3 CHR Language Features Across Implementations

| Feature | SWI | SICStus | Haskell | FreeCHR |
|---------|-----|---------|---------|---------|
| Simplification | ✓ | ✓ | ✓ | ✓ |
| Propagation | ✓ | ✓ | ✓ | ✓ |
| Simpagation | ✓ | ✓ | ✓ | ✓ |
| Guards | ✓ | ✓ | ✓ | ✓ |
| Multi-headed | ✓ | ✓ | ✓ | ✓ |
| Disjunction | Limited | Yes | Limited | Yes |
| Priorities | ✓ | ✓ | ✓ | ✓ |
| Tabling | ✓ | Limited | Limited | ✓ |

---

## 9. How CHR Relates to Evident's Design

### 9.1 Current Evident Model

Evident is built on:
1. **Schemas**: Named sets defined by membership constraints
2. **Z3 backend**: Constraints compile to SMT, solver finds models
3. **Independent queries**: Each schema query is solved independently

```
Query: (p ∈ Person, p.age > 30)
       → Person schema constraints + Z3 solver
       → Model: {name: "Alice", age: 35, role: "Manager"}
```

### 9.2 CHR-Inspired Extensions

Three levels of integration:

#### Level 1: Intra-Schema Rule Systems (No Change to Z3)
Use CHR-style rules *within* a schema to simplify before passing to Z3:

```
schema Person {
  age ∈ Nat
  senior ∈ {true, false}
  
  % Pre-process: derive `senior` constraint from age before Z3
  age >= 65 → senior = true
}
```

Implementation: Preprocess the query constraints with CHR before sending to Z3.

#### Level 2: Inter-Schema Communication (New)
Allow rules that trigger queries across schemas:

```
schema Employee {
  id ∈ Nat
  manager_id ∈ Nat
  salary ∈ Nat
}

schema Budget {
  department_id ∈ Nat
  allocated ∈ Nat
}

% When an employee is assigned to a department with insufficient budget, flag it
rule check_budget:
  (e ∈ Employee, e.manager_id = M) 
    → (b ∈ Budget, b.department_id = M)
        [assert: b.allocated >= e.salary]
```

Implementation:
- Employee schema query derives new constraint on `manager_id`
- This triggers Budget schema query with that `department_id`
- Results propagate back (refinement of Employee model)

#### Level 3: Distributed Schema Networks (Research)
Deploy schemas on multiple nodes, rules propagate constraints across nodes:

```
% Global constraint: all salaries across departments consistent
node1.schema Employee | node2.schema Employee | node3.schema Employee
  → check salary_consistency across all nodes
```

Implementation: Similar to CHRe, with Z3 solver per node.

### 9.3 Key Questions for Evident

1. **Should schemas be confluent?**
   - If yes: inter-schema rules must form confluent rule set
   - If no: query results depend on execution order (acceptable for some domains)

2. **Should inter-schema rules be part of Evident syntax?**
   - Could be separate "rule" construct: `rule name: (schema1_pattern) → (schema2_pattern) [guard]`
   - Or use existing `when`/`then` structure in language design

3. **How does fixpoint iteration interact with Z3?**
   - Each schema query is a "big step" (Z3 solves a batch of constraints)
   - Inter-schema rules are "small steps" (add constraints one at a time)
   - Could cause performance issues if many rounds of refinement

4. **Error handling for unsatisfiability?**
   - If schema query returns UNSAT, how do other schemas respond?
   - Should triggers be transactional?

---

## 10. Concrete CHR Examples

### 10.1 GCD Solver

The classic example:

```
gcd(N, N) <=> 
  % Base case: GCD of N and N is N
  true.

gcd(X, Y) <=> 
  X > Y |
  gcd(X - Y, Y).

gcd(X, Y) <=> 
  Y > X |
  gcd(X, Y - X).
```

Execution:
```
Query: gcd(18, 12)

Store: {gcd(18, 12)}
Rule 2 applies (18 > 12): 
  gcd(18, 12) → gcd(6, 12)

Store: {gcd(6, 12)}
Rule 3 applies (12 > 6):
  gcd(6, 12) → gcd(6, 6)

Store: {gcd(6, 6)}
Rule 1 applies:
  gcd(6, 6) → true

Store: {} (or: store contains "true" constraint, computation succeeds)
Result: GCD(18, 12) = 6
```

**Key observations**:
- No explicit recursion/backtracking needed
- Rules rewrite constraints in the store
- Termination guaranteed (arguments decrease monotonically)
- Only one possible result (confluence)

### 10.2 Sorting Network (Odd-Even Sort)

Constraint: given a list of numbers, propagate comparisons until sorted.

```
% Constraints represent: elem(Pos, Value)
% Rules: if elem(I, V1) and elem(I+1, V2) and V1 > V2, swap them

elem(I, V1), elem(I+1, V2) <=> 
  V1 > V2 | 
  elem(I, V2), elem(I+1, V1).

% Redundancy: if two adjacent elements have the same constraint, remove duplicate
elem(I, V), elem(I, V) <=> elem(I, V).
```

Execution on list [3, 1, 4, 1, 5]:
```
Store: {elem(0,3), elem(1,1), elem(2,4), elem(3,1), elem(4,5)}

Fire: elem(0,3) > elem(1,1)
Store: {elem(0,1), elem(1,3), elem(2,4), elem(3,1), elem(4,5)}

Fire: elem(1,3) > elem(2,4)? No.

Fire: elem(2,4) > elem(3,1)? Yes.
Store: {elem(0,1), elem(1,3), elem(2,1), elem(3,4), elem(4,5)}

... (more rounds)

Final: {elem(0,1), elem(1,1), elem(2,3), elem(3,4), elem(4,5)}
Result: [1, 1, 3, 4, 5] ✓
```

### 10.3 Type Inference (Hindley-Milner Fragment)

Simplified HM type checking in CHR:

```
% Constraint: var X has type T
:- chr_constraint type/2, unify_types/2.

% Rule 1: Reflexivity
type(X, T), type(X, T) <=> type(X, T).

% Rule 2: If X has two different types, unify them
type(X, T1), type(X, T2) <=> 
  T1 \= T2 |
  unify_types(T1, T2), type(X, T1).

% Rule 3: Propagate: if f: A→B and f applied to A, result is B
type(F, arrow(ArgType, RetType)), 
type(Arg, ArgType) ==>
type(app(F, Arg), RetType).

% Rule 4: Resolve: unification constraints
unify_types(T, T) <=> true.
unify_types(arrow(A1, B1), arrow(A2, B2)) <=>
  unify_types(A1, A2), unify_types(B1, B2).
```

For expression `(\x. x + 1)`:
```
% Lambda expression analysis
Constraints:
  type(x, ?T)           % variable x has unknown type
  type(+, Int → Int → Int)  % + is a function
  type(1, Int)          % constant 1 is Int

Rule 3 propagates:
  type(+, Int → Int → Int), type(1, Int) ==>
  type(plus_1, Int → Int)

Rule 3 propagates again:
  type(plus_1, Int → Int), type(x, ?T) & ?T = Int ==>
  type(x + 1, Int)

Result: The entire expression has type Int → Int
```

Advantages over Hindley-Milner algorithm:
- No explicit constraint generation phase + unification phase
- Rules fire incrementally as types are discovered
- Confluent execution ⟹ unique inferred type independent of discovery order

---

## 11. Lessons for Evident

### 11.1 What to Adopt

1. **Multi-headed constraint matching**: When querying across schemas, match multiple constraints simultaneously
   ```
   schema Person { id, dept }
   schema Budget { dept, amount }
   
   Query: Person(id=1, dept=D), Budget(dept=D) 
          → returns both matched constraints
   ```

2. **Guard conditions on query patterns**: Test properties before instantiating Z3 solver
   ```
   Query: Person(p), p.age > 65 | p.dept = "HR"
          → Only query Z3 if age > 65, then check dept constraint
   ```

3. **Propagation history**: Avoid re-querying the same schema combination twice
   ```
   If we've already unified Person(1) with Budget("Engineering"),
   don't re-query that pair if it appears elsewhere
   ```

4. **Termination proofs**: For multi-schema rules, design so iterations decrease
   ```
   Each schema query should either:
   - Add fewer constraints (terminating)
   - Add constraints at a higher "level" (stratified)
   ```

### 11.2 What to Avoid

1. **Replacing Z3 with CHR rewriting**: Z3 is more powerful for arithmetic, decision procedures
   - Keep Z3 for within-schema solving
   - Use CHR-style rules only for inter-schema coordination

2. **Unbounded rule cycles**: Prevent infinite loops between schemas
   ```
   BAD: Person rule triggers Budget rule triggers Company rule ... → Person rule again
   
   GOOD: Person → Budget (one direction), no back-edge
   ```

3. **Implicit execution order**: Always document rule firing order expectations
   ```
   % Execution order matters even in confluent programs (performance)
   % Document which rules should fire first
   ```

### 11.3 Integration Path

**Phase 1** (Current): Single-schema queries with Z3
- Evident as designed

**Phase 2** (Proposed): Multi-schema composition without rules
- Explicit composition syntax: `Person(p) | Budget(b)`
- Manual constraint threading between schemas

**Phase 3** (Future): Inter-schema rules
- New syntax for rules firing across schemas
- Automatic constraint propagation
- Fixpoint computation engine

**Phase 4** (Research): Distributed schema networks
- Deploy schemas on multiple nodes
- Rules propagate constraints globally
- Eventual consistency semantics

---

## 12. References & Implementation Resources

### Core Literature

- [Constraint Handling Rules (Cambridge University Press)](https://www.cambridge.org/core/books/constraint-handling-rules/1172E6B9A6BC650CBF3A0EDC19F48A94) — Authoritative textbook on CHR theory and practice
- [Theory and practice of constraint handling rules](https://www.sciencedirect.com/science/article/pii/S0743106698100055) — Foundational paper on CHR semantics
- [Constraint Handling Rules – Compilation, Execution, and Analysis](https://exia.informatik.uni-ulm.de/fruehwirth/chr_thesis_book-free-download.pdf) — Comprehensive thesis on CHR implementation
- [CHR Wikipedia](https://en.wikipedia.org/wiki/Constraint_Handling_Rules) — Overview and references

### Confluence and Termination

- [On Termination, Confluence and Consistent CHR-based Type Inference](https://arxiv.org/abs/1405.3393) — Termination analysis techniques
- [On Proving Confluence Modulo Equivalence for Constraint Handling Rules](https://ar5iv.labs.arxiv.org/html/1611.03628) — Confluence proof methods
- [Improved Termination Analysis of CHR Using Self-sustainability Analysis](https://link.springer.com/chapter/10.1007/978-3-642-32211-2_13) — Advanced termination checking

### Distributed & Multi-Agent

- [Decentralized Execution of Constraint Handling Rules for Ensembles](http://reports-archive.adm.cs.cmu.edu/anon/qatar/CMU-CS-QTR-118.pdf) — CHRe framework for distributed systems
- [Cooperating Constraint Solvers](https://link.springer.com/chapter/10.1007/3-540-45349-0_42) — Multi-solver coordination

### Modern Frameworks

- [FreeCHR – An Algebraic Framework for Constraint Handling Rules Embeddings](https://arxiv.org/html/2306.00642) — Formalization of CHR embedding
- [An instance of FreeCHR with refined operational semantics](https://arxiv.org/pdf/2505.22155) — Recent refinements to CHR semantics

### Implementation Documentation

- **SWI-Prolog CHR**: https://www.swi-prolog.org/pldoc/man?section=chr-intro
- **SICStus Prolog CHR**: https://sicstus.sics.se/sicstus/docs/latest/html/sicstus/CHR-Introduction.html
- **CHR Language Overview**: http://eclipseclp.org/doc/libman/libman053.html

### Key Workshops & Communities

- **CHR Workshop** (bi-annual, affiliated with major logic programming venues)
- **DTAI KU Leuven CHR project**: https://dtai.cs.kuleuven.be/projects/CHR/

---

## 13. Design Decisions for Evident

### Decision 1: Include Inter-Schema Rules?

**Option A: No** (Current design)
- Schemas are independent constraint systems
- Users compose schemas explicitly
- Simpler semantics, easier to reason about

**Option B: Yes** (CHR-inspired)
- Add `rule` construct for inter-schema constraints
- Automatic triggering and propagation
- More powerful but more complex

**Recommendation**: Start with **Option A**, add **Option B** in a future phase if users request it.

### Decision 2: Rule Syntax

If adopting inter-schema rules, syntax options:

```
% Option 1: Separate rule construct
rule consistency_check:
  person(P) ∧ P.manager_id = M
    ⇒ manager(M) must exist
  
% Option 2: Extend existing schema syntax
schema Person {
  when manager_id = M:
    trigger: manager(M) ∈ Manager schema
}

% Option 3: New composition operator
Person(p) →→ Manager(m) when p.manager_id = m.id
```

**Recommendation**: **Option 1** is clearest (separates concerns).

### Decision 3: Fixpoint Iteration Strategy

How to handle multi-round queries:

```
% Naive: Iterate queries until fixpoint
loop {
  results_A = query_A(...)
  results_B = query_B(results_A)
  if results_A ≠ new_results_A: continue
  if results_B ≠ new_results_B: continue
  break
}

% Optimized: Use tabling to avoid re-querying
tabled_results_A = {}
tabled_results_B = {}
loop {
  new_A = query_A(...) - tabled_results_A
  if new_A empty: break
  tabled_results_A += new_A
  
  new_B = query_B(new_A) - tabled_results_B
  tabled_results_B += new_B
}
```

**Recommendation**: Start with optimized version, add aggressive caching.

---

## Conclusion

Constraint Handling Rules offers a mature, well-studied model for building cooperating constraint solvers through forward-chaining, multi-headed rules. The key innovations—committed choice, propagation history, confluence-based correctness—are orthogonal to Evident's Z3 backend and could enhance Evident's expressiveness for multi-schema systems.

For immediate application to Evident:
1. Study CHR's confluence and termination techniques for future rule systems
2. Consider multi-schema composition syntax that mirrors CHR's multi-headed matching
3. Plan a roadmap for inter-schema rules in Evident (Phase 3+)
4. Document execution semantics clearly so users understand rule firing order

The confluence and termination properties provide a path to **proven correctness** of complex constraint systems—a goal that aligns with Evident's vision of constraint programming as a declarative specification language.

---

**Document prepared**: April 30, 2026  
**For**: Evident language design team  
**Next steps**: Prototype inter-schema rule system (Phase 3 planning)
