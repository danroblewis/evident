# Set Operations in Constraint Programming Systems

Research for the Evident language design project. The goal is to understand what set operations constraint programmers actually need, based on what existing CP systems provide natively versus what they leave programmers to derive.

---

## 1. MiniZinc Set Operations

MiniZinc has first-class set types as a language primitive. A set is an unordered collection of distinct values drawn from a finite domain. Set variables (`var set of T`) are genuine decision variables — the solver must determine which elements they contain.

### Set Type Declarations

```minizinc
% Constant set (known at model construction time)
set of int: allowed = {1, 3, 5, 7, 9};

% Set variable (solver decides which elements it contains)
var set of 1..10: chosen;

% Set variable constrained to a subdomain
var set of allowed: selection;

% Array of sets
array[1..3] of var set of 1..5: groups;
```

### Set Operations and Syntax

| Operation | MiniZinc Syntax | Mathematical Meaning |
|---|---|---|
| Membership | `x in S` | `x ∈ S` |
| Non-membership | `not (x in S)` | `x ∉ S` |
| Union | `S union T` | `S ∪ T` |
| Intersection | `S intersect T` | `S ∩ T` |
| Difference | `S diff T` | `S \ T` |
| Subset | `S subset T` | `S ⊆ T` |
| Superset | `S superset T` | `S ⊇ T` |
| Equality | `S = T` | `S = T` |
| Cardinality | `card(S)` | `|S|` |
| Symmetric diff | `(S diff T) union (T diff S)` | `S △ T` (derived) |

### Set Comprehensions

```minizinc
% Set comprehension: {x | x in 1..10, x mod 2 = 0}
{x | x in 1..10 where x mod 2 = 0}

% Set from array
{a[i] | i in index_set(a) where pred(a[i])}
```

### Cardinality Constraints

```minizinc
% Exactly k elements
card(S) = k;

% At most k elements
card(S) <= k;

% Non-empty
card(S) >= 1;
```

### Arrays of Sets

```minizinc
% Partition: cover all elements, no overlap
constraint forall(i, j in 1..n where i != j)(
    groups[i] intersect groups[j] = {}
);
constraint union(i in 1..n)(groups[i]) = universe;
```

### What Works Well in MiniZinc Sets

MiniZinc's set constraints are propagated efficiently when the underlying solver supports set variables (e.g., Gecode). Operations like `in`, `subset`, `card`, `union`, and `intersect` have native propagators. More complex derived expressions may be decomposed into element-level constraints.

When set variables are not supported by the backend solver, MiniZinc decomposes set constraints into arrays of Boolean indicator variables (`x_in_S[i] = 1` iff element `i` is in set `S`), and rewrites all set operations as integer/Boolean constraints. This is transparent to the programmer but changes solver performance characteristics significantly.

---

## 2. SICStus Prolog CLP(Set) and Set Constraints

SICStus Prolog includes `library(clpfd)` for finite-domain integers, but set constraints in Prolog-family systems are typically handled through:

1. **CLPSET** (Azevedo & Barahona) — a constraint system over finite sets with symbolic set variables
2. **CONJUNTO** — early CLP(Set) system by Gervet (1994), one of the first practical implementations

### Core Constraints in CLP(Set)

```prolog
% Membership constraint
in(X, S)             % X ∈ S
nin(X, S)            % X ∉ S

% Subset
subset(S, T)         % S ⊆ T

% Disjointness
disjoint(S, T)       % S ∩ T = ∅

% Union
union(S, T, U)       % U = S ∪ T

% Intersection
intersection(S, T, U) % U = S ∩ T

% Difference
difference(S, T, U)  % U = S \ T

% Cardinality
#(S, N)              % N = |S| (N is a CLP(FD) variable)
```

### Set Variable Representation

In CONJUNTO and related systems, a set variable is represented by two bounds:
- **glb** (greatest lower bound): elements definitely in the set
- **lub** (least upper bound): elements possibly in the set

A set variable `S` satisfies `glb(S) ⊆ S ⊆ lub(S)`. Propagation proceeds by:
- Adding elements to `glb` when membership is forced
- Removing elements from `lub` when exclusion is forced

This is the standard **interval-based domain representation** for set variables. It is exact for the standard set constraints but may not capture all dependencies (the "bound consistency" vs "arc consistency" tradeoff for sets is an active research area).

### Practical Status

CONJUNTO influenced later work but is not widely used in production. Most practitioners use MiniZinc with a Gecode or OR-Tools backend, or encode sets as arrays of Booleans in CLP(FD).

---

## 3. Gecode Set Constraints

Gecode is one of the most complete and well-documented CP toolkits, and has first-class `SetVar` support with a rich propagator library.

### SetVar Domains

A `SetVar` in Gecode is bounded by `[glb, lub]`: the greatest lower bound (elements definitely in the set) and least upper bound (elements possibly in the set). Propagation narrows this interval.

```cpp
// Create a set variable over universe {1..10}
SetVar s(home, IntSet::empty, IntSet(1, 10));

// Set variable constrained to contain exactly 3 elements
SetVar s(home, IntSet::empty, IntSet(1, 10), 3, 3); // cardinality bounds
```

### Gecode Set Propagators

**Membership and element:**
```cpp
dom(home, s, SRT_SUP, x);     // x ∈ s (element must be in set)
dom(home, s, SRT_DISJ, x);    // x ∉ s (exclude element)
```

**Relational constraints between sets (`rel`):**
```cpp
rel(home, s, SRT_EQ, t);      // s = t
rel(home, s, SRT_NQ, t);      // s ≠ t
rel(home, s, SRT_SUB, t);     // s ⊆ t
rel(home, s, SRT_SUP, t);     // s ⊇ t
rel(home, s, SRT_DISJ, t);    // s ∩ t = ∅
rel(home, s, SRT_CMPL, t);    // s = complement of t
```

**Set operations producing a new set:**
```cpp
rel(home, s, SOT_UNION, t, u);   // u = s ∪ t
rel(home, s, SOT_INTER, t, u);   // u = s ∩ t
rel(home, s, SOT_DUNION, t, u);  // u = s ⊎ t (disjoint union)
rel(home, s, SOT_MINUS, t, u);   // u = s \ t
```

**Cardinality:**
```cpp
cardinality(home, s, 2, 5);      // 2 ≤ |s| ≤ 5
cardinality(home, s, n);         // |s| = n (IntVar)
```

**Minimum and maximum element:**
```cpp
min(home, s, x);   // x = min(s) — x is an IntVar
max(home, s, x);   // x = max(s)
```

**Element of array of sets:**
```cpp
// s = array[idx]
element(home, setarray, idx, s);
```

**Channel between set variable and array of Booleans:**
```cpp
// b[i] = 1 iff i ∈ s
channel(home, boolarray, s);

// Channel between SetVar and IntVar array
channel(home, intarray, s);
```

**Convex sets:**
```cpp
convex(home, s);               // s = {min(s)..max(s)} (no gaps)
convex(home, s, t);            // t is the convex hull of s
```

**Sequence (ordered sets):**
```cpp
sequence(home, setarray);       // sets form consecutive sequence
```

**Set-valued element:**
```cpp
selectUnion(home, setarray, idx_set, result);  // result = ∪{array[i] | i ∈ idx_set}
selectInter(home, setarray, idx_set, result);  // result = ∩{array[i] | i ∈ idx_set}
selectDisjoint(home, setarray, idx_set);       // sets indexed by idx_set are pairwise disjoint
```

### Propagation Quality in Gecode

Gecode's set propagators are generally **bound-consistent**: they propagate through the `[glb, lub]` interval. Most set constraints have native propagators that avoid decomposition. The `channel` constraint linking `SetVar` to `BoolVar` arrays is particularly important for hybrid models where you want both set-level reasoning and element-level reasoning.

---

## 4. Global Constraints Involving Sets

Global constraints are high-level constraints over collections of variables that have efficient native propagators. Many global constraints have a set-theoretic reading.

### `all_different`

**Mathematical meaning:** The variables `x₁, ..., xₙ` take pairwise distinct values — i.e., the multiset `{x₁, ..., xₙ}` has no duplicates, equivalently `|{x₁, ..., xₙ}| = n`.

**Set-theoretic reading:** The function `i ↦ xᵢ` is injective.

**Propagator:** The classic algorithm by Régin (1994) uses bipartite matching. If every maximal matching must include edge `(i, v)`, then `xᵢ = v` is forced. If no maximum matching includes edge `(i, v)`, then `v` can be removed from `dom(xᵢ)`. This is **arc-consistent** and runs in O(n^1.5) time per invocation.

**Common use:** Ubiquitous. Appears in nearly every combinatorial puzzle and assignment problem.

### `partition`

**Mathematical meaning:** A collection of sets `S₁, ..., Sₙ` partitions universe `U` iff:
1. `S₁ ∪ ... ∪ Sₙ = U` (coverage)
2. `Sᵢ ∩ Sⱼ = ∅` for all `i ≠ j` (disjointness)

**MiniZinc:**
```minizinc
constraint partition_set(groups, 1..n);
% or equivalently:
constraint forall(i,j in 1..k where i<j)(groups[i] intersect groups[j] = {});
constraint union(i in 1..k)(groups[i]) = 1..n;
```

**Propagator:** There is a native `partition_set` global in MiniZinc/Gecode. Propagation works by tracking which elements are unassigned to any partition and which elements appear in multiple partitions.

**Common use:** Graph coloring, timetabling, bin packing, scheduling with non-overlapping groups.

### `disjoint`

**Mathematical meaning:** `S ∩ T = ∅` — no shared elements.

**Propagator:** Direct: any element forced into `S` is excluded from `T`, and vice versa. Arc-consistent.

**Common use:** Non-overlap constraints (e.g., no two tasks share a resource simultaneously, no two intervals overlap).

### `among`

**Mathematical meaning:** Given a set of values `V` and variables `x₁, ..., xₙ`, the number of variables with a value in `V` is exactly `k`:
```
|{i | xᵢ ∈ V}| = k
```

Can also express as `k ∈ [lo, hi]` for a range.

**MiniZinc:**
```minizinc
constraint among(k, x, allowed_values);
% Equivalent to:
constraint k = sum(i in 1..n)(bool2int(x[i] in allowed_values));
```

**Propagator:** The `among` constraint has a native propagator that achieves bound consistency on `k`. When `k` is fixed and few values are unassigned, propagation can force specific assignments or exclusions.

**Common use:** Crew scheduling (exactly 2 pilots per flight), shift scheduling (at most 3 night shifts per week), pattern constraints (at least one of type X in each window).

### `global_cardinality` (GCC)

**Mathematical meaning:** For each value `v` in a specified set, the number of variables in `x₁, ..., xₙ` that take value `v` is constrained:
```
|{i | xᵢ = v}| = cᵥ  (or cᵥ ∈ [loᵥ, hiᵥ])
```

**MiniZinc:**
```minizinc
constraint global_cardinality(x, [1,2,3], [c1, c2, c3]);
% c1 = number of variables equal to 1, etc.
```

**Propagator:** Régin's GCC propagator (2000) extends bipartite matching. Achieves arc consistency. One of the most sophisticated and widely-implemented global constraints.

**Common use:** Scheduling (each shift staffed by exactly the right number), assignment problems with capacity limits, bin packing.

### `exactly` / `at_most` / `at_least`

Special cases of `among` and `global_cardinality`:

```minizinc
% Exactly k variables equal to v
constraint exactly(k, x, v);

% At most k variables in set V
constraint count(x, V) <= k;
```

### `at_most_one`

**Mathematical meaning:** At most one variable in `x₁, ..., xₙ` takes value 1 (or satisfies a predicate). A special case of `among` with `k ≤ 1`.

**Common use:** Mutual exclusion, choice constraints, scheduling.

---

## 5. ILOG CP Optimizer / OR-Tools Set Support

### ILOG CP Optimizer (IBM)

CP Optimizer is an industrial CP solver with strong scheduling support. It does not expose first-class set variables in the way Gecode does, but set-based reasoning is available through:

- **`IloAllDiff`** — all-different over an array of integer variables
- **`IloAllDiffInt`** — same
- **Allowed/forbidden assignments** — `IloTableConstraint` specifies which combinations of values are allowed (a relation, i.e., a set of tuples)
- **`IloDistribute`** (global cardinality) — how many variables take each value
- **Cumulative functions** — track resource usage over time (a form of cardinality over intervals)

Set-level constraints in CP Optimizer are typically expressed through:
1. Integer variable arrays with `alldiff` / `distribute`
2. Boolean encoding (`IloBoolVarArray`) with sum constraints
3. Interval variables for scheduling with non-overlap (`IloNoOverlap`)

### OR-Tools CP-SAT

OR-Tools' CP-SAT solver encodes everything in terms of Boolean and integer variables. Set constraints are handled through:

```python
from ortools.sat.python import cp_model

model = cp_model.CpModel()

# "x in allowed_values" becomes:
model.AddAllowedAssignments([x], [[v] for v in allowed_values])

# Cardinality: exactly k variables in set V
indicators = [model.NewBoolVar(f'b{i}') for i in range(n)]
for i, xi in enumerate(x):
    model.AddLinearExpressionInDomain(
        xi, cp_model.Domain.FromValues(allowed_values)
    ).OnlyEnforceIf(indicators[i])
model.Add(sum(indicators) == k)

# All-different
model.AddAllDifferent(x)

# Global cardinality
counts = [model.NewIntVar(0, n, f'c{v}') for v in values]
model.AddGlobalCardinality(x, values, counts)
```

OR-Tools CP-SAT does not expose `SetVar`; it encodes set membership as Boolean indicator variables. However, the `AddAllDifferent`, `AddGlobalCardinality`, and `AddAllowedAssignments` constraints have efficient native implementations using propagators similar to Régin's algorithms.

**OR-Tools explicitly provides:**
- `AddAllDifferent`
- `AddGlobalCardinality`
- `AddAllowedAssignments` / `AddForbiddenAssignments` (tabular constraints)
- `AddLinearConstraint` with `Domain.FromValues` for membership
- `AddNoOverlap` / `AddNoOverlap2D` for interval/rectangle scheduling (set-theoretic non-overlap)
- `AddCumulative` for resource capacity (cardinality over time)

---

## 6. Set Constraints in CHR (Constraint Handling Rules)

CHR is a rule-based language for writing constraint solvers. It is not a solver itself but a framework for implementing custom constraint systems. You can define set constraints by writing propagation and simplification rules.

### Implementing Set Membership in CHR

```prolog
% CHR rules for a simple set constraint store
:- use_module(library(chr)).

:- chr_constraint in/2, nin/2, subset/2, union/3, intersection/3.

% Propagation: if x in S and y in S and S = {x}, then y = x
subset(S, T), in(X, T) ==> in(X, S).

% Simplification: redundant membership
in(X, S), in(X, S) <=> in(X, S).

% Simplification: contradictory constraints
in(X, S), nin(X, S) <=> fail.

% Propagation for union: if X in A then X in (A union B)
in(X, A), union(A, B, C) ==> in(X, C).
in(X, B), union(A, B, C) ==> in(X, C).

% Propagation for intersection
in(X, C), intersection(A, B, C) ==> in(X, A), in(X, B).
nin(X, A), intersection(A, B, C) ==> nin(X, C).
nin(X, B), intersection(A, B, C) ==> nin(X, C).
```

### What CHR Gives You

CHR's strengths for set constraints:
- **Flexibility**: You define exactly the propagation rules you want
- **Modularity**: Set constraint rules compose with other CHR constraints
- **Prototyping**: Ideal for experimenting with new constraint types

CHR's weaknesses:
- No automatic arc consistency — you must write rules to achieve it
- Performance is proportional to how carefully you write the rules
- No bound-domain representation (glb/lub) unless you implement it explicitly

CHR is widely used for research into new constraint types and for teaching constraint programming. It is less commonly used in production compared to MiniZinc or Gecode. The `SICStus Prolog` and `SWI-Prolog` CHR libraries are the standard implementations.

---

## 7. Primitives vs. Derived Operations

This is the key engineering question for any CP system supporting sets.

### Truly Native (Efficient Native Propagators Exist)

These have mature, well-studied propagators that achieve arc consistency or bound consistency efficiently:

| Constraint | Propagator basis | Complexity |
|---|---|---|
| `x ∈ S` (fixed S) | Domain filtering | O(1) per value removed |
| `S ⊆ T` | glb/lub propagation | O(\|lub\|) |
| `S ∩ T = ∅` (disjoint) | glb/lub propagation | O(\|glb\|) |
| `card(S) = k` | Counting on lub/glb | O(\|lub\|) |
| `all_different` | Bipartite matching (Régin) | O(n^1.5) |
| `global_cardinality` | Flow-based matching (Régin) | O(n·k) |
| `union(A, B) = C` | Three-way glb/lub | O(\|lub\|) |
| `inter(A, B) = C` | Three-way glb/lub | O(\|lub\|) |

### Typically Derived

These are expressible in terms of primitive operations. Good systems provide them as sugar but may not give them specialized propagators:

| Constraint | Derived from |
|---|---|
| `S diff T = U` | `U = S ∩ complement(T)`, or: `x ∈ U ↔ x ∈ S ∧ x ∉ T` |
| `symmetric_diff(S, T) = U` | `U = (S diff T) union (T diff S)` |
| `among(k, x, V)` | `k = sum(bool2int(xᵢ ∈ V))` + cardinality propagation |
| `partition(S₁,...,Sₙ, U)` | `pairwise disjoint + union = U` |
| `S = ∅` | `card(S) = 0` |
| `S ⊊ T` (strict subset) | `S ⊆ T ∧ S ≠ T` |
| `exactly(k, x, v)` | `among(k, x, {v})` |
| `convex(S)` | `min(S)..max(S) = S` (range check) |

### The Bool-Encoding Fallback

When a solver does not support `SetVar` natively (OR-Tools CP-SAT, most SMT solvers), set constraints are encoded as:

```
S encoded as Boolean array b[1..n] where b[i] = 1 iff i ∈ S

x ∈ S   →   b[x] = 1
|S| = k  →   sum(b) = k
S ⊆ T   →   forall i: b_S[i] <= b_T[i]
S ∪ T   →   forall i: b_U[i] = max(b_S[i], b_T[i])
S ∩ T   →   forall i: b_I[i] = min(b_S[i], b_T[i])
```

This encoding is exact but loses the compact set-level reasoning. Propagation quality depends on how well the solver handles the resulting Boolean/integer constraints.

---

## 8. The `among` Constraint in Depth

`among` deserves special attention because it bridges set membership and cardinality counting — two fundamental ideas in constraint programming.

### Formal Definition

Given:
- Variables `x₁, ..., xₙ` over integer domains
- A set of values `V ⊆ ℤ`
- An integer variable (or constant) `k`

The constraint `among(k, x, V)` holds iff:
```
k = |{i ∈ 1..n | xᵢ ∈ V}|
```

### Propagation

**From `k` to variables:** If `k = n` (all variables must take values in `V`), then for each `xᵢ`, remove all values not in `V` from `dom(xᵢ)`. If `k = 0`, remove all values in `V` from each `dom(xᵢ)`.

**From variables to `k`:**
- `lo = |{i | dom(xᵢ) ⊆ V}|` — variables already forced to take a value in V
- `hi = |{i | dom(xᵢ) ∩ V ≠ ∅}|` — variables that could take a value in V
- Propagate: `lo ≤ k ≤ hi`

This achieves **bound consistency** on `k`. Full arc consistency requires more expensive reasoning.

### Variants

```minizinc
% Exactly k
constraint among(k, x, V);

% At least lo, at most hi
constraint lo <= count(x, V) /\ count(x, V) <= hi;

% Sliding window variant: in any consecutive window of w variables,
% at least lo and at most hi take values in V
constraint forall(i in 1..n-w+1)(
    among_window(lo, hi, x[i..i+w-1], V)
);
```

The **sliding window `among`** (also called `sequence` or `among_seq`) is critical in workforce scheduling: "in any 7 consecutive days, each employee works between 3 and 5 days." It has a native propagator in some systems (e.g., SICStus CLP(FD)).

### Real-World Uses

- **Crew scheduling**: At least 1 captain and at most 2 co-pilots per flight (2 separate `among` constraints)
- **Nurse rostering**: Each nurse works between 3 and 5 shifts per week out of the available shift types
- **Bin packing with type constraints**: Each bin contains at most 2 items of type X
- **Pattern scheduling**: No more than 3 consecutive night shifts

---

## 9. Set Covering, Packing, and Partitioning

These three classic problem families are the canonical application of set constraints in combinatorial optimization. They are variants of the same basic structure.

### Shared Setup

Given a universe `U = {1, ..., m}` and a collection of subsets `S₁, ..., Sₙ ⊆ U`, select a subcollection `T ⊆ {S₁, ..., Sₙ}`.

### Set Covering

**Definition:** Every element of `U` appears in at least one selected set.
```
∀ u ∈ U, ∃ Sᵢ ∈ T : u ∈ Sᵢ
```

**Integer programming encoding:**
```
For each j ∈ 1..n: yⱼ ∈ {0,1} (1 = Sⱼ selected)
For each u ∈ U:   sum{j | u ∈ Sⱼ} yⱼ  ≥  1
Objective:        minimize sum yⱼ  (or minimize total cost)
```

**CP encoding (MiniZinc):**
```minizinc
array[1..n] of var 0..1: selected;
constraint forall(u in universe)(
    sum(j in 1..n where u in S[j])(selected[j]) >= 1
);
```

**Applications:** Emergency facility location (every city covered by at least one hospital), minimum test set (every bug caught by at least one test), antenna placement.

### Set Packing

**Definition:** No element of `U` appears in more than one selected set (selected sets are pairwise disjoint).
```
∀ i ≠ j : Sᵢ ∩ Sⱼ = ∅  (for Sᵢ, Sⱼ ∈ T)
```

**Integer programming encoding:**
```
For each j ∈ 1..n: yⱼ ∈ {0,1}
For each u ∈ U:   sum{j | u ∈ Sⱼ} yⱼ  ≤  1
Objective:        maximize sum yⱼ  (or maximize total profit)
```

**Applications:** Maximum independent set (packing non-adjacent vertices), frequency assignment (packing non-interfering broadcasts), matching.

### Set Partitioning

**Definition:** Every element appears in exactly one selected set.
```
∀ u ∈ U, ∃! Sᵢ ∈ T : u ∈ Sᵢ
```

**Integer programming encoding:**
```
For each j ∈ 1..n: yⱼ ∈ {0,1}
For each u ∈ U:   sum{j | u ∈ Sⱼ} yⱼ  =  1
```

This is the conjunction of covering and packing — both the `≥ 1` and `≤ 1` constraints. Equivalently: the selected sets form a partition of `U`.

**Applications:** Crew pairing (each flight covered by exactly one crew), vehicle routing (each customer served by exactly one route), graph coloring (each node in exactly one color class).

### CP Encoding with `partition_set`

```minizinc
% Direct partition constraint in MiniZinc
constraint partition_set(groups, 1..m);

% Equivalent explicit encoding
constraint forall(i, j in 1..k where i < j)(
    groups[i] intersect groups[j] = {}   % packing: disjoint
);
constraint union(i in 1..k)(groups[i]) = 1..m;  % covering: complete
```

### Relationship to `among` and Global Cardinality

Set covering/packing/partitioning are special cases of GCC:
- Covering: each element's count ≥ 1
- Packing: each element's count ≤ 1
- Partitioning: each element's count = 1

The `global_cardinality` constraint subsumes all three when the "values" are elements and the "variables" are set memberships.

---

## Summary: Operations by Frequency and Importance

### Tier 1: Appear in Almost Every Set-Based CP Model

| Operation | Description | Native propagator? |
|---|---|---|
| `x ∈ S` | Membership test / constraint | Yes (trivial) |
| `S ⊆ T` | Subset relationship | Yes (glb/lub) |
| `S ∩ T = ∅` | Disjointness | Yes (glb/lub) |
| `\|S\| = k` | Cardinality | Yes |
| `all_different(x)` | Distinct values | Yes (matching) |
| `partition(S, U)` | Partition a universe | Yes (in most systems) |

### Tier 2: Common, Usually Have Efficient Implementations

| Operation | Description | Native propagator? |
|---|---|---|
| `S ∪ T = U` | Union | Yes (glb/lub) |
| `S ∩ T = I` | Intersection | Yes (glb/lub) |
| `among(k, x, V)` | Count variables in V | Yes (bound consistency) |
| `global_cardinality` | Count per value | Yes (matching) |
| `S diff T = D` | Set difference | Sometimes; often decomposed |
| `min(S)`, `max(S)` | Extremes of a set | Yes (in Gecode) |

### Tier 3: Useful but Often Decomposed

| Operation | Description | Notes |
|---|---|---|
| `symmetric_diff(S, T)` | Elements in exactly one of S, T | Decomposed from union + diff |
| `convex(S)` | S has no gaps | Native in Gecode; decomposed elsewhere |
| `selectUnion(sets, idx, result)` | Union of selected sets | Native in Gecode |
| `sliding_among` | Among over consecutive windows | Native in SICStus; decomposed in others |
| `exactly(k, x, v)` | Exactly k variables equal v | Special case of among |

---

## Key Takeaways for Evident

1. **Set variables with glb/lub domains** are the standard representation. The interval `[glb, lub]` where `glb ⊆ S ⊆ lub` is how every mature CP system represents uncertainty about a set. This is the analogue of `[lo, hi]` for integer variables.

2. **Cardinality is fundamental.** `card(S)` appears constantly, both as a standalone constraint and embedded in `among`, `partition`, and `global_cardinality`. Any language with sets needs cardinality as a first-class operation.

3. **The `among` constraint is the right abstraction** for many real problems. It combines membership testing with counting — "how many of these things satisfy this set-valued predicate?" — in a way that has an efficient propagator.

4. **Disjointness and partition are the workhorse set constraints.** Most combinatorial structure in scheduling, assignment, and covering problems reduces to "these things don't overlap" and "these things cover everything."

5. **Boolean indicator encoding** is always available as a fallback. `b[i] = 1 iff i ∈ S` reduces set constraints to integer/Boolean arithmetic, which SMT solvers handle well. But it loses the compact propagation that native set constraint solvers provide.

6. **`all_different` is a set constraint in disguise.** It says the range of a function is the same size as its domain — a purely set-theoretic condition. Its efficient propagator (Régin's algorithm) is one of the most important results in constraint programming.

7. **MiniZinc has the most complete set syntax** of any widely-used modeling language. Its set operations (`union`, `intersect`, `diff`, `subset`, `superset`, `card`, `in`) and set comprehensions are a reasonable reference point for what a CP modeling language should surface to programmers.
