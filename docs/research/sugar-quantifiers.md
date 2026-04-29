# Syntactic Sugar for Quantified Assertions Over Collections

Research for the Evident constraint programming language design project. Goal: survey how programming languages, specification languages, query languages, and formal methods tools express "for all elements of a set, this constraint holds" — and what syntax is most readable, composable, and appropriate for Evident.

---

## 1. `any` / `all` / `none` / `count` in Programming Languages

The four basic quantifiers over a collection appear in some form in virtually every modern language. They correspond directly to the logical quantifiers ∃ and ∀ and their negations.

### Python

```python
any(P(x) for x in S)       # ∃ x ∈ S : P(x)
all(P(x) for x in S)       # ∀ x ∈ S : P(x)
not any(P(x) for x in S)   # ¬∃ x ∈ S : P(x)  — "none"
sum(1 for x in S if P(x))  # |{x ∈ S | P(x)}|  — count matching
```

Python uses generator expressions, which are syntactically very close to set-builder notation. `any(f(x) for x in S)` reads almost like `∃ x ∈ S : f(x)`. There is no built-in `none()` or `count(predicate)` — programmers use `not any(...)` and `sum(1 for x in S if P(x))` or `len(list(filter(P, S)))`.

Key properties:
- Short-circuiting: `any` stops at the first true; `all` stops at the first false.
- Works on any iterable, not just sets.
- Generator expression syntax `(expr for var in iterable if condition)` is the closest Python gets to set comprehension.

### Ruby

```ruby
S.any? { |x| P(x) }    # ∃ x ∈ S : P(x)
S.all? { |x| P(x) }    # ∀ x ∈ S : P(x)
S.none? { |x| P(x) }   # ¬∃ x ∈ S : P(x)
S.one? { |x| P(x) }    # |{x ∈ S | P(x)}| = 1
S.count { |x| P(x) }   # |{x ∈ S | P(x)}|
```

Ruby's `Enumerable` module provides all four quantifiers as first-class methods. The trailing block syntax `{ |x| ... }` separates the collection from the predicate. The `?` suffix signals a boolean-returning method — a strong naming convention.

Notable: Ruby includes `one?` as a primitive, expressing unique satisfaction. No other mainstream language provides this as a first-class method.

### Haskell

```haskell
any P xs    -- ∃ x ∈ xs : P(x)
all P xs    -- ∀ x ∈ xs : P(x)
```

`any` and `all` take a predicate function and a `Foldable`. Since Haskell functions are curried, `any P` is a function `Foldable t => t a -> Bool`, making it composable. There is no `none` (programmers write `not . any P`) and no `count` predicate form (programmers write `length . filter P` or use `Data.List.Foldable`).

Haskell's `any`/`all` syntax is the closest to mathematical function application: `any P xs` reads almost exactly like "any P of xs" where P is a first-class function. The predicate is not a lambda in a block — it is a fully first-class value.

### JavaScript

```javascript
S.every(x => P(x))    // ∀ x ∈ S : P(x)
S.some(x => P(x))     // ∃ x ∈ S : P(x)
S.find(x => P(x))     // the first x ∈ S satisfying P (or undefined)
S.filter(x => P(x)).length  // |{x ∈ S | P(x)}|
```

JavaScript uses `every`/`some` rather than `all`/`any`. The naming is slightly more English-prose-like (`every element satisfies...`, `some element satisfies...`). `find` returns the first witness, not a boolean. There is no `none` or `count(predicate)` primitive.

### Scala

```scala
S.forall(P)     // ∀ x ∈ S : P(x)
S.exists(P)     // ∃ x ∈ S : P(x)
S.count(P)      // |{x ∈ S | P(x)}|
S.find(P)       // Option[A] — first witness
```

Scala uses `forall`/`exists`, the vocabulary of formal logic. This is a closer mapping to the mathematical symbols than Python's `all`/`any` or JavaScript's `every`/`some`. `count(predicate)` is a first-class method.

### Comparison to Formal Logic

| Language form | Formal equivalent | Notes |
|---|---|---|
| `all(P(x) for x in S)` | `∀ x ∈ S : P(x)` | Python; generator syntax reads naturally |
| `S.all? { \|x\| P(x) }` | `∀ x ∈ S : P(x)` | Ruby; block syntax |
| `all P xs` | `∀ x ∈ S : P(x)` | Haskell; point-free, most compact |
| `S.every(x => P(x))` | `∀ x ∈ S : P(x)` | JS; "every" is clearer English than "all" |
| `S.forall(P)` | `∀ x ∈ S : P(x)` | Scala; most logically named |
| `any(P(x) for x in S)` | `∃ x ∈ S : P(x)` | Python |
| `S.any? { \|x\| P(x) }` | `∃ x ∈ S : P(x)` | Ruby |
| `any P xs` | `∃ x ∈ S : P(x)` | Haskell |
| `S.some(x => P(x))` | `∃ x ∈ S : P(x)` | JS; "some" is natural English |
| `S.exists(P)` | `∃ x ∈ S : P(x)` | Scala; logical vocabulary |

**Observation for Evident:** The `forall`/`exists` naming (Scala, OCL, MiniZinc) is the clearest mapping to the formal semantics. The Haskell `any P xs` style — predicate as first argument, collection as second — makes chaining and partial application natural. The Python `all(P(x) for x in S)` style puts the quantifier as a function applied to a generator, which reads well for humans unfamiliar with formal logic. All three approaches have merits; the key tension is between **readability for non-experts** and **precision for formal-reasoning contexts**.

---

## 2. SQL's `EXISTS` and `NOT EXISTS`

SQL lacks `∀` as a native operator but supports `∃` through `EXISTS`. Universal quantification is expressed as double negation — the standard DeMorgan encoding.

### Existential quantification

```sql
-- ∃ employee in the set who works in Engineering
SELECT d.name
FROM departments d
WHERE EXISTS (
    SELECT 1 FROM employees e
    WHERE e.dept_id = d.id
      AND e.title = 'Engineer'
)
```

`EXISTS (subquery)` returns true if the subquery produces at least one row. The subquery's output is irrelevant; only whether it is empty matters — hence `SELECT 1` is idiomatic.

### Universal quantification via double negation

SQL has no `FORALL` operator. To express `∀ x ∈ S : P(x)` in SQL, the standard pattern is:

```sql
-- ∀ project : this employee has an assignment for it
-- = there is no project for which this employee has no assignment
SELECT e.id
FROM employees e
WHERE NOT EXISTS (
    SELECT 1 FROM projects p
    WHERE NOT EXISTS (
        SELECT 1 FROM assignments a
        WHERE a.employee_id = e.id
          AND a.project_id = p.id
    )
)
```

This double-negation pattern encodes the DeMorgan equivalence:

```
∀ x ∈ S : P(x)
  ≡ ¬∃ x ∈ S : ¬P(x)
  ≡ NOT EXISTS (x ∈ S WHERE NOT P(x))
```

The double negation is necessary because SQL's closed-world assumption makes `NOT EXISTS` well-defined: a subquery that finds no rows is provably empty (not "unknown").

### The DeMorgan transformation

This transformation is the bridge between existential and universal quantification in any language:

| Logical form | DeMorgan equivalent | SQL encoding |
|---|---|---|
| `∀ x : P(x)` | `¬∃ x : ¬P(x)` | `NOT EXISTS (WHERE NOT P)` |
| `∃ x : P(x)` | `¬∀ x : ¬P(x)` | `EXISTS (WHERE P)` |
| `none x : P(x)` | `∀ x : ¬P(x)` | `NOT EXISTS (WHERE P)` |
| `some x : P(x)` | `¬none x : P(x)` | `EXISTS (WHERE P)` |

**Practical problem:** The double-`NOT EXISTS` pattern is widely regarded as one of SQL's most opaque constructs. It requires readers to mentally chain two negations — a known cognitive difficulty. The alternative (using `COUNT` or `ALL`) is sometimes clearer but has its own pitfalls.

### Scalar subquery comparisons

SQL's `ALL` and `ANY`/`SOME` are alternative spellings for quantified comparisons:

```sql
-- salary > ALL values in subquery  (i.e., salary is the maximum)
SELECT name FROM employees WHERE salary > ALL (SELECT salary FROM managers)

-- salary > ANY value in subquery  (i.e., salary exceeds at least one manager's)
SELECT name FROM employees WHERE salary > ANY (SELECT salary FROM managers)
```

These are limited to scalar comparisons and cannot express arbitrary predicates. They are also rarely used in practice — `NOT IN` and `EXISTS` patterns dominate.

---

## 3. SQL's `HAVING` Clause

`HAVING` expresses constraints over groups after aggregation. It is structurally identical to `WHERE`, but operates on the output of `GROUP BY` rather than individual rows.

```sql
-- Groups (departments) with more than 5 employees
SELECT department, COUNT(*) AS headcount
FROM employees
GROUP BY department
HAVING COUNT(*) > 5
```

### HAVING as a cardinality constraint

`HAVING COUNT(*) > k` is a cardinality constraint on a group: "this set has more than k elements." Common patterns:

```sql
HAVING COUNT(*) > 1       -- duplicates: at least two rows share this group key
HAVING COUNT(*) = 1       -- unique: exactly one row
HAVING COUNT(DISTINCT x) = COUNT(x)  -- no duplicates in column x
HAVING COUNT(*) = (SELECT COUNT(*) FROM S)  -- all elements of S are present
```

The last pattern is a common idiom for division (relational ÷) — "the group contains all elements of another set."

### WHERE vs. HAVING

| Clause | Operates on | Can reference aggregates | Timing |
|---|---|---|---|
| `WHERE` | Individual rows | No | Before GROUP BY |
| `HAVING` | Groups (post-aggregation) | Yes | After GROUP BY |

The key insight is that `HAVING` is a filter on sets (groups), while `WHERE` is a filter on elements (rows). This two-level structure maps directly to the constraint programming distinction between:
- Element-level constraints (on each member of a set)
- Set-level constraints (on the set as a whole — its cardinality, aggregate value, etc.)

**Implication for Evident:** Evident should distinguish between element constraints (`for all x in S: P(x)`) and set constraints (`|S| > k`, `sum(S) < budget`). SQL's WHERE/HAVING distinction is the right model for this.

---

## 4. MiniZinc: Constraints as First-Class Functions

MiniZinc is the most widely used constraint modeling language. Its quantifiers are function calls — `forall` and `exists` are not syntax but first-class constraint generators.

### Basic quantifiers

```minizinc
% ∀ i ∈ S : P(i)
constraint forall(i in S)(P(i));

% ∃ i ∈ S : P(i)
constraint exists(i in S)(P(i));

% Count of elements satisfying P is at least k
constraint count(i in S, P(i)) >= k;
```

`forall(i in S)(P(i))` generates the conjunction of `P(i)` for all `i` in `S`. It desugars to `P(s1) /\ P(s2) /\ ... /\ P(sn)` for concrete sets, or the appropriate constraint network for variable-extent sets.

### Array comprehensions and generators

MiniZinc uses generators — expressions of the form `expr where condition` — in array/set comprehensions and quantifiers:

```minizinc
% All elements of a 2D matrix satisfy a constraint
constraint forall(i in 1..n, j in 1..n where i != j)(x[i] != x[j]);

% Sum of elements satisfying a condition
int: total = sum(i in 1..n where active[i])(cost[i]);

% Existential over a filtered set
constraint exists(i in 1..n where eligible[i])(assigned[i] = task);
```

The `where` clause inside a generator is a guard — it restricts the range. This is equivalent to `∀ i ∈ {i ∈ S | guard(i)} : P(i)`.

### `count` as a primitive

```minizinc
% Exactly k elements satisfy P
constraint count(i in S, P(i)) = k;

% At most k
constraint count(i in S, P(i)) <= k;

% At least k
constraint count(i in S, P(i)) >= k;
```

`count` in MiniZinc is a global constraint — solvers can exploit its structure for propagation, rather than treating it as a sum of booleans. This is an important design principle: cardinality constraints deserve special status, not just syntactic sugar.

### Global constraints as library functions

MiniZinc includes a rich library of global constraints that express common combinatorial patterns:

```minizinc
constraint alldifferent(x);            % all elements of array x are distinct
constraint alldifferent_except_0(x);   % alldifferent, ignoring zeros
constraint atmost(k, x, v);            % at most k elements of x equal v
constraint atleast(k, x, v);           % at least k elements of x equal v
constraint exactly(k, x, v);           % exactly k elements of x equal v
constraint cumulative(start, dur, res, cap);  % resource scheduling
constraint element(i, x, v);          % x[i] = v (array element access)
```

The design philosophy: common constraint patterns should be global constraints with efficient propagators, not just syntactic encodings.

**Key insight for Evident:** MiniZinc's approach — quantifiers as functions generating constraints, with generators that can carry guards — is probably the best model for a constraint language. The `forall(i in S where guard(i))(P(i))` pattern is both readable and compositional.

---

## 5. Alloy: First-Class Logical Quantifiers

Alloy is a formal specification language based on first-order relational logic. Its quantifiers are part of the core logic, not library functions.

### The five Alloy quantifiers

```alloy
all x : S | P[x]     -- ∀ x ∈ S : P(x)
some x : S | P[x]    -- ∃ x ∈ S : P(x)
no x : S | P[x]      -- ¬∃ x ∈ S : P(x)  (none)
one x : S | P[x]     -- ∃! x ∈ S : P(x)  (unique existence)
lone x : S | P[x]    -- |{x ∈ S | P(x)}| ≤ 1  (at most one)
```

Alloy's quantifier vocabulary is richer than any programming language:
- `all` — universal (∀)
- `some` — existential (∃)
- `no` — universal negation (∀ x : ¬P, or equivalently ¬∃ x : P)
- `one` — unique existence (∃!)
- `lone` — "at most one" — zero or one (no standard logic symbol)

### The `lone` quantifier

`lone` is Alloy's most distinctive contribution. It means "zero or one" — i.e., the set of witnesses has cardinality 0 or 1. This is weaker than `one` (which requires exactly 1) but stronger than `some` (which requires at least 1). It expresses optionality with uniqueness:

```alloy
lone x : Person | x.role = "Manager"  -- at most one person is a Manager
one x : Person | x.role = "CEO"       -- exactly one person is CEO
```

`lone` is important for modeling partial functions, optional assignments, and "at most one" cardinality constraints — a common need in constraint programming.

### Multiple bindings

Alloy allows multiple variables in a single quantifier, similar to nested quantifiers:

```alloy
all x, y : Person | x != y => x.name != y.name    -- names are unique
all x : S, y : T | related[x][y]                  -- cross-product quantification
```

### Set expressions

Alloy's quantifiers range over set expressions, not just base relations:

```alloy
all x : S & T | P[x]           -- over intersection
all x : S - T | P[x]           -- over difference
all x : S.field | P[x]         -- over relational image
all x : S | x in T => P[x]     -- conditional (guarded quantifier)
```

The conditional form `x in T => P[x]` inside a universal quantifier is equivalent to a guarded quantifier — common in practice when you want to restrict the range without constructing the filtered set explicitly.

**Key insight for Evident:** Alloy's `lone` and `one` quantifiers deserve adoption. "Exactly one" and "at most one" are common constraint patterns (unique assignments, bijections, optional relationships) that should have first-class syntax rather than being encoded as `count(...) = 1` or `count(...) <= 1`. The naming `no` for "none" is more readable than `not any` or `not exists`.

---

## 6. TLA+: Quantifiers in Temporal Specifications

TLA+ (Temporal Logic of Actions) is Leslie Lamport's specification language for concurrent and distributed systems. It uses standard mathematical notation, ASCII-encoded.

### Basic quantifiers

```tla
\A x \in S : P(x)    -- ∀ x ∈ S : P(x)
\E x \in S : P(x)    -- ∃ x ∈ S : P(x)
```

`\A` is ASCII for ∀ (for All); `\E` is ASCII for ∃ (there Exists). The notation `x \in S` reads "x in S."

TLA+ tools (TLC model checker, TLAPS proof system) display these using the actual Unicode symbols ∀ and ∃ in their output.

### Bounded vs. unbounded quantifiers

TLA+ distinguishes between quantifiers with an explicit domain (`\A x \in S`) and unbounded quantifiers (`\A x`) over the entire mathematical universe. Only bounded quantifiers are decidable by TLC; unbounded quantifiers require the proof system.

```tla
\A x \in 1..n : P(x)      -- bounded: checkable by model checker
\A x \in Nat : P(x)        -- bounded but infinite: not checkable
\A x : P(x)                -- unbounded: proof only
```

### Quantification over tuples and higher-order sets

```tla
\A <<x, y>> \in S \X T : P(x, y)   -- over Cartesian product
\A f \in [S -> T] : P(f)            -- over function spaces
\E S \in SUBSET Base : P(S)         -- over subsets of Base
```

The last form — `\E S \in SUBSET Base` — is second-order existential quantification over subsets, which TLC handles by enumeration for finite `Base`.

### CHOOSE: selecting a witness

TLA+ has a unique `CHOOSE` operator for non-deterministically picking an element:

```tla
CHOOSE x \in S : P(x)    -- some x ∈ S satisfying P (undefined if none exists)
```

`CHOOSE` is the TLA+ analog of Hilbert's epsilon operator — it picks a canonical witness. Unlike `\E`, it returns a value, not a boolean. This is useful for defining functions that select an element from a set satisfying some property.

**Observation for Evident:** The ASCII `\A`/`\E` convention is a practical compromise for typing mathematical symbols. Evident should prefer Unicode `∀`/`∃` in source if the toolchain supports it, with `forall`/`exists` as the English-keyword fallback. TLA+'s `CHOOSE` is interesting — an expression form of existential quantification that returns the witness rather than a boolean.

---

## 7. OCL: Object Constraint Language

OCL is the constraint language embedded in UML. It uses a collection-oriented pipeline style where operations are chained with `->`. OCL's collection operations are among the most comprehensive in any specification language.

### Quantifiers

```ocl
S->forAll(x | P(x))     -- ∀ x ∈ S : P(x)
S->exists(x | P(x))     -- ∃ x ∈ S : P(x)
S->isUnique(x | f(x))   -- all values of f(x) are distinct for x ∈ S
```

OCL's `isUnique` is interesting: it is equivalent to `S->forAll(x, y | x <> y implies f(x) <> f(y))` — i.e., `f` is injective on `S`. This is a common constraint (no two elements map to the same value) that deserves its own name.

### Selection and filtering

```ocl
S->select(x | P(x))     -- {x ∈ S | P(x)} — subset satisfying P
S->reject(x | P(x))     -- {x ∈ S | ¬P(x)} — subset rejecting P
S->collect(x | f(x))    -- {f(x) | x ∈ S} — image under f (with duplicates)
```

`collect` is OCL's `map`. Unlike mathematical image, it preserves duplicates (because OCL collections include bags, not just sets).

### Cardinality and membership

```ocl
S->size()               -- |S|
S->isEmpty()            -- |S| = 0
S->notEmpty()           -- |S| > 0
S->includes(x)          -- x ∈ S
S->excludes(x)          -- x ∉ S
S->includesAll(T)       -- T ⊆ S
S->excludesAll(T)       -- S ∩ T = ∅
```

### Aggregation

```ocl
S->sum()                -- ∑_{x ∈ S} x  (requires numeric elements)
S->count(x)             -- |{y ∈ S | y = x}|  (frequency of element x)
```

Note: OCL's `count(x)` counts occurrences of a specific value, not elements satisfying a predicate. To count elements satisfying a predicate, use `S->select(x | P(x))->size()`.

### Any and one

```ocl
S->any(x | P(x))        -- some x ∈ S satisfying P (returns the element, not boolean)
S->one(x | P(x))        -- |{x ∈ S | P(x)}| = 1 (boolean: exactly one)
```

OCL's `any` is a selector (returns the element, like TLA+'s `CHOOSE`), while `one` is a boolean quantifier meaning "exactly one." The naming creates potential confusion with Alloy's `one` (which is a quantifier, not a predicate).

### The collection hierarchy

OCL distinguishes four collection types:
- `Set` — unordered, no duplicates
- `Bag` — unordered, duplicates allowed (multiset)
- `Sequence` — ordered, duplicates allowed (list)
- `OrderedSet` — ordered, no duplicates

Most quantifiers work uniformly across all four types. The distinction matters for `collect` (which may produce duplicates) vs. `select` (which preserves the collection type).

**Key insights for Evident:**
- `isUnique` is a valuable primitive — injective mapping constraints are common.
- The separation of `any` (a selector returning a value) from quantifiers (returning booleans) is important to get right.
- OCL's pipeline style `S->forAll(x | ...)` with the collection on the left and the quantifier as a method is readable but has the downside of making the collection syntactically primary, when often the constraint is the point of interest.

---

## 8. Z Notation

Z is a formal specification language based on typed set theory. Its notation influenced many subsequent specification languages (including Alloy and OCL) and the mathematical notation in type theory.

### Quantifiers in Z

Z uses standard mathematical symbols, with schemas as types:

```z
∀ x : S • P(x)     -- universal: for all x in type S, P(x) holds
∃ x : S • P(x)     -- existential: there exists x in type S satisfying P(x)
∃₁ x : S • P(x)    -- unique existential: exactly one x satisfies P(x)
```

The bullet `•` separates the variable binding from the predicate, similar to the `:` separator in natural deduction.

### Schema comprehension

The key contribution of Z is the *schema* — a record type with invariants built in:

```z
Student == [name : Name; age : ℕ | age ≥ 18]
```

A schema `[x₁ : T₁; x₂ : T₂ | P(x₁, x₂)]` defines a set of records satisfying P. This is set comprehension at the type level — the invariant is part of the type, not an external constraint.

### Set comprehension and filtering

```z
{x : S | P(x)}        -- {x ∈ S | P(x)} — set comprehension
{x : S | P(x) • f(x)} -- {f(x) | x ∈ S, P(x)} — image under f of filtered set
```

### Influence on programming languages

Z's notation influenced:
- **Haskell**: List comprehensions `[f x | x <- xs, P x]` follow Z's `{f(x) | x ∈ S • P(x)}` exactly, with `<-` for ∈ and `,` for `•`.
- **Python**: `[f(x) for x in S if P(x)]` is the same pattern.
- **LINQ** (C#/F#): `from x in S where P(x) select f(x)` is Z-style comprehension in SQL-like syntax.
- **Scala**: `for { x <- S if P(x) } yield f(x)` follows Z.

The universality of the comprehension pattern `{f(x) | x ∈ S, P(x)}` — with variable binding, domain, optional guard, and result expression — across languages shows it is the fundamental idiom for expressing quantification in programming.

**Key insight for Evident:** The Z-style schema comprehension — where invariants are embedded in type definitions — is a powerful idea. Rather than writing `∀ x ∈ Students : x.age ≥ 18`, a type `Students` could carry the invariant internally. This is relevant to Evident's model of claims: a claim type might embed its invariants as part of its definition.

---

## 9. SPARQL: Graph Quantification and Filters

SPARQL is the query language for RDF graphs. Its quantification model is distinctive because the "set" being quantified over is a graph pattern, not a collection.

### FILTER: element-level constraints

```sparql
SELECT ?person WHERE {
  ?person rdf:type :Person .
  ?person :age ?age .
  FILTER(?age >= 18)
}
```

`FILTER` is a predicate applied to each solution (binding of variables). It is the SPARQL analog of SQL's `WHERE` — element-level constraint.

### EXISTS and NOT EXISTS

```sparql
-- ∃ project that this person manages
SELECT ?person WHERE {
  ?person rdf:type :Person .
  FILTER EXISTS { ?person :manages ?project }
}

-- ¬∃ project managed (i.e., person manages nothing)
SELECT ?person WHERE {
  ?person rdf:type :Person .
  FILTER NOT EXISTS { ?person :manages ?project }
}
```

SPARQL's `FILTER EXISTS { pattern }` and `FILTER NOT EXISTS { pattern }` are existential and universal quantification over graph patterns. `NOT EXISTS` in SPARQL is used to express "no solutions exist matching this pattern" — i.e., universal negation.

### OPTIONAL: existential with fallback

```sparql
SELECT ?person ?email WHERE {
  ?person rdf:type :Person .
  OPTIONAL { ?person :email ?email }
}
```

`OPTIONAL` is an outer join — it includes the solution whether or not the optional pattern matches, filling unmatched variables with an unbound state. This is existential quantification with a default when the existential is false.

### HAVING in SPARQL

SPARQL 1.1 supports `GROUP BY` and `HAVING`, paralleling SQL:

```sparql
SELECT ?dept (COUNT(?person) AS ?headcount) WHERE {
  ?person rdf:type :Person .
  ?person :department ?dept .
}
GROUP BY ?dept
HAVING (COUNT(?person) > 5)
```

`HAVING` in SPARQL is a cardinality constraint on groups, identical semantics to SQL.

### Aggregate quantification with `NOT EXISTS`

SPARQL can express "for all members of the set matching pattern P1, they also match pattern P2" using `NOT EXISTS`:

```sparql
-- All employees in Engineering have signed the NDA
ASK {
  FILTER NOT EXISTS {
    ?emp :department "Engineering" .
    FILTER NOT EXISTS { ?emp :signed :NDA }
  }
}
```

This double `NOT EXISTS` is the SPARQL encoding of the DeMorgan-transformed universal quantifier — identical in structure to SQL's double `NOT EXISTS` pattern.

**Observation for Evident:** SPARQL's treatment of `OPTIONAL` as first-class syntax for "maybe-exists" is interesting. In constraint programming, the concept of an optional constraint (one that applies only if a guard is satisfied) is common. SPARQL names this explicitly.

---

## 10. Property-Based Testing: Universal Properties

Property-based testing libraries frame testing as quantification: a property test is a claim that `∀ x ∈ Domain : P(x)`.

### QuickCheck (Haskell)

```haskell
-- Property: sorting is idempotent
prop_sort_idempotent :: [Int] -> Bool
prop_sort_idempotent xs = sort (sort xs) == sort xs

-- Running: QuickCheck generates 100 random inputs and checks P(x)
quickCheck prop_sort_idempotent

-- With explicit universal quantifier
forAll (listOf arbitrary) $ \xs -> sort (sort xs) == sort xs
```

QuickCheck's `forAll gen prop` directly mirrors `∀ x ∈ Domain : P(x)` where `gen` specifies the domain (generator) and `prop` is the predicate. The `forAll` function is named to evoke the universal quantifier explicitly.

### ScalaCheck

```scala
import org.scalacheck.Prop._

// ∀ xs : List[Int] : sort(sort(xs)) = sort(xs)
forAll { (xs: List[Int]) =>
  xs.sorted.sorted == xs.sorted
}

// With explicit generator
forAll(Gen.listOf(Gen.choose(0, 100))) { xs =>
  xs.sorted.sorted == xs.sorted
}
```

ScalaCheck uses `forAll` with type-directed generation — the generator is inferred from the type when possible.

### Hypothesis (Python)

```python
from hypothesis import given, strategies as st

@given(st.lists(st.integers()))
def test_sort_idempotent(xs):
    assert sorted(sorted(xs)) == sorted(xs)
```

Hypothesis uses a decorator `@given(domain)` where `domain` is a strategy (generator). The property `∀ xs ∈ Lists[Int] : P(xs)` is expressed as `@given(st.lists(st.integers()))` followed by a function body that asserts `P(xs)`.

### The connection to formal quantification

Property-based testing is essentially empirical verification of universal quantified statements:

```
QuickCheck/Hypothesis tests:    ∀ x ∈ Domain : P(x)
Formal verification proves:     ∀ x ∈ Domain : P(x)
```

The difference is that testing samples the domain (and finds counterexamples), while formal verification exhausts it (or proves the claim holds for all inputs).

This connection has practical implications:
- A property-testing library's `forAll(gen) { x => P(x) }` syntax is a natural model for Evident's quantified constraints.
- The generator (`gen`) corresponds to Evident's set/domain — the binding `x in S`.
- The predicate body corresponds to Evident's constraint body.

**Key insight for Evident:** Property-based testing libraries converged on `forAll(domain) { x => predicate }` as the natural syntax for universal quantification in programs. This is essentially `forall(x in S)(P(x))` — the MiniZinc syntax. The fact that two independent traditions (formal specification and property testing) arrived at the same structure validates it.

---

## 11. The `unique` / `one` / `exactly one` Quantifier

"Exactly one element of S satisfies P" is a common constraint that most languages handle clumsily.

### Formal notation

`∃! x ∈ S : P(x)` — the "unique existence" quantifier (read: "there exists a unique x...").

It is equivalent to:
```
∃ x ∈ S : P(x)  ∧  ∀ y ∈ S : P(y) ⟹ y = x
```
or, via counting:
```
|{x ∈ S | P(x)}| = 1
```

### How languages express it

| Language | "Exactly one" syntax | Notes |
|---|---|---|
| Ruby | `S.one? { \|x\| P(x) }` | First-class method |
| Alloy | `one x : S \| P[x]` | First-class quantifier |
| OCL | `S->one(x \| P(x))` | First-class operation |
| MiniZinc | `count(i in S, P(i)) = 1` | Via count constraint |
| Python | `sum(1 for x in S if P(x)) == 1` | Verbose |
| Haskell | `length (filter P xs) == 1` | Verbose; not short-circuiting |
| SQL | `(SELECT COUNT(*) FROM S WHERE P) = 1` | Verbose; subquery |
| Prolog | *not standard* | `aggregate_all(count, P(X), 1)` in SWI |

### Importance for constraint programming

"Exactly one" constraints appear constantly in combinatorial problems:
- **Bijection**: every element of S maps to a unique element of T
- **Assignment**: every task is assigned to exactly one worker
- **Covering**: every region is covered by exactly one set
- **Uniqueness**: a key value appears exactly once

Encoding these as `count(...) = 1` is possible but verbose and requires the solver to propagate through the count constraint. Languages that provide `one` as a primitive (Alloy, OCL, Ruby) make such constraints more readable and potentially enable specialized propagators.

### The `lone` quantifier (zero or one)

Alloy's `lone` — "at most one" — is equally important:
- **Optional unique assignment**: a slot may have 0 or 1 assignments
- **Partial function**: `f(x)` is defined for at most one `x` in a set
- **Optional match**: a record may match at most one rule

```alloy
lone x : Worker | assigned[x]     -- at most one worker is assigned
one x : Worker | is_lead[x]       -- exactly one lead worker
```

No other mainstream language has `lone` as a primitive. It is typically encoded as `count(...) <= 1` or `not (some x, y | x != y and P(x) and P(y))`.

**Recommendation for Evident:** Include `one` and `lone` (or `atmost`) as first-class quantifiers alongside `all`, `some`/`exists`, and `no`/`none`. These cover the five cardinality bands:
- `no` / `none`: count = 0
- `lone` / `atmost one`: count ≤ 1
- `one` / `exactly one`: count = 1
- `some` / `exists`: count ≥ 1
- `all` / `forall`: all elements satisfy P

---

## 12. Chained and Nested Quantifiers

Expressing `∀ x ∈ S, ∀ y ∈ T : P(x, y)` is a common need — for example, "for every pair of students in the same class, their IDs are distinct."

### Flat multi-variable syntax

Several languages allow multiple bindings in a single quantifier:

**Alloy:**
```alloy
all x, y : Student | x != y => x.id != y.id
```

**MiniZinc:**
```minizinc
constraint forall(i in 1..n, j in 1..n where i < j)(x[i] != x[j]);
```

**TLA+:**
```tla
\A x \in S, y \in T : P(x, y)
```

**SQL (equivalent):**
```sql
NOT EXISTS (
    SELECT 1 FROM students s1, students s2
    WHERE s1.class = s2.class
      AND s1.id = s2.id
      AND s1.student_no != s2.student_no
)
```

### Nested quantifiers

When the second range depends on the first variable, flat syntax is unavailable and nesting is required:

**Alloy:**
```alloy
all c : Class | all s : c.students | s.gpa >= 2.0
```

**MiniZinc:**
```minizinc
constraint forall(c in Classes)(
    forall(s in students_in[c])(gpa[s] >= 2.0)
);
```

**Python:**
```python
all(s.gpa >= 2.0 for c in classes for s in c.students)
```

Python's chained generator expression handles this naturally: `for c in classes for s in c.students` is a nested iteration. It is equivalent to:
```python
all(s.gpa >= 2.0 for c in classes for s in c.students if condition)
```
which is `∀ c ∈ classes, ∀ s ∈ c.students : s.gpa ≥ 2.0`.

### Readability challenges

Nested quantifiers create cognitive load because readers must track:
1. Which variable is bound by which quantifier
2. Whether the inner range depends on the outer variable
3. Whether the quantifiers are the same type or mixed (∀∃ vs. ∀∀)

`∀ x ∈ S : ∃ y ∈ T : P(x, y)` (everyone has a match) is fundamentally different from `∃ y ∈ T : ∀ x ∈ S : P(x, y)` (one element matches everyone). These look similar but have opposite semantics.

### Solutions in language design

**Explicit labels:** Alloy's `all c : Class | all s : c.students` makes the nesting structure explicit — each quantifier names its variable.

**Flat comma syntax with type annotations:** `forall(i in S, j in T where i < j)` reduces nesting when ranges are independent.

**Comprehension syntax:** Python's `for x in S for y in T(x)` makes dependent iteration feel natural, and the generator expression syntax `all(P for x in S for y in T(x))` reads almost like prose.

**Guard syntax:** Rather than nested `if`, a single top-level guard `where i != j` after the bindings reduces nesting depth.

**Recommendation for Evident:**
- Support multi-variable flat syntax when ranges are independent: `forall x in S, y in T | P(x, y)`
- Support chained binding when the second range depends on the first: `forall x in S | forall y in x.related | P(x, y)` — making the dependency explicit through nesting
- Allow guards on any binding: `forall x in S, y in S where x != y | P(x, y)` for the common "all pairs" pattern
- Avoid the Python flat-generator approach for nested quantifiers, as it obscures the dependency structure

---

## Comparison Table: Quantifier Syntax Across Languages

| Quantifier | Python | Ruby | Haskell | JavaScript | MiniZinc | Alloy | OCL | SQL |
|---|---|---|---|---|---|---|---|---|
| **∀ (forall)** | `all(P(x) for x in S)` | `S.all? { \|x\| P }` | `all P xs` | `S.every(P)` | `forall(x in S)(P(x))` | `all x : S \| P[x]` | `S->forAll(x\|P)` | `NOT EXISTS (WHERE NOT P)` |
| **∃ (exists)** | `any(P(x) for x in S)` | `S.any? { \|x\| P }` | `any P xs` | `S.some(P)` | `exists(x in S)(P(x))` | `some x : S \| P[x]` | `S->exists(x\|P)` | `EXISTS (WHERE P)` |
| **none** | `not any(P(x) for x in S)` | `S.none? { \|x\| P }` | `not (any P xs)` | `!S.some(P)` | `forall(x in S)(not P(x))` | `no x : S \| P[x]` | `S->select(x\|P)->isEmpty()` | `NOT EXISTS (WHERE P)` |
| **exactly one** | `sum(1 for x in S if P(x))==1` | `S.one? { \|x\| P }` | `length (filter P xs)==1` | `S.filter(P).length==1` | `count(x in S, P(x))=1` | `one x : S \| P[x]` | `S->one(x\|P)` | `(SELECT COUNT(*) WHERE P)=1` |
| **at most one** | `sum(1 for x in S if P(x))<=1` | — | `length (filter P xs)<=1` | `S.filter(P).length<=1` | `count(x in S, P(x))<=1` | `lone x : S \| P[x]` | `S->one(x\|P) or S->select(x\|P)->isEmpty()` | `(SELECT COUNT(*) WHERE P)<=1` |
| **count** | `sum(1 for x in S if P(x))` | `S.count { \|x\| P }` | `length (filter P xs)` | `S.filter(P).length` | `count(x in S, P(x))` | `#{ x : S \| P[x] }` | `S->select(x\|P)->size()` | `SELECT COUNT(*) WHERE P` |
| **select (filter)** | `[x for x in S if P(x)]` | `S.select { \|x\| P }` | `filter P xs` | `S.filter(P)` | `[x \| x in S where P(x)]` | `{ x : S \| P[x] }` | `S->select(x\|P)` | `WHERE P` |
| **first witness** | `next((x for x in S if P(x)),None)` | `S.find { \|x\| P }` | `find P xs` | `S.find(P)` | — | — | `S->any(x\|P)` | — |

### Readability assessment

| Style | Readability for experts | Readability for non-experts | Composability |
|---|---|---|---|
| MiniZinc `forall(x in S)(P(x))` | High | High | High (function call) |
| Alloy `all x : S \| P[x]` | High | Medium | High |
| Python `all(P(x) for x in S)` | Medium | High | Low (not composable) |
| Ruby `S.all? { \|x\| P(x) }` | Medium | High | Medium |
| Haskell `all P xs` | High (experts) | Low | Very high (curried) |
| SQL `NOT EXISTS (WHERE NOT P)` | Low | Very low | Low |

---

## Synthesis: Design Recommendations for Evident

From this survey, several principles emerge:

### 1. Use `forall`/`exists`/`no`/`one`/`lone` as first-class quantifiers

The Alloy set of five quantifiers covers the cardinality spectrum cleanly:
- `no` (count = 0) is clearer than `not any` or `not exists`
- `lone` (count ≤ 1) is the only common cardinality band without a good name in most languages
- `one` (count = 1) should be a primitive, not `count(...) = 1`

### 2. Put the binding before the predicate

The `forall(x in S)(P)` or `forall x in S | P` style is consistently more readable than the collection-method style `S.forAll(x | P)`, because:
- The constraint is the focus, not the collection
- Multiple bindings compose naturally: `forall x in S, y in T | P(x, y)`
- The syntax mirrors formal logic

### 3. Support generator guards for filtering

`forall x in S where guard(x) | P(x)` is cleaner than filtering the set before quantifying: `forall x in select(S, guard) | P(x)`. The inline guard keeps the binding, restriction, and body together.

### 4. Distinguish element constraints from set constraints

Following SQL's WHERE/HAVING model:
- `forall x in S | P(x)` — element constraint: applies to each element
- `|S| > k`, `sum(S) > budget` — set constraints: apply to the set as a whole

Both are constraints in Evident's model, but they operate at different levels and benefit from different syntax.

### 5. Provide `count` as a primitive, not derived

`count(x in S | P(x)) >= k` is a cardinality constraint that solvers can exploit specially. Encoding it as `|{x ∈ S | P(x)}| >= k` (filter then size) loses this optimization opportunity.

### 6. Make `any` a selector, `exists` a quantifier

Following OCL's distinction:
- `exists(x in S | P(x))` — boolean: true if any element satisfies P
- `any(x in S | P(x))` — value: the element itself (or undefined/error if none)

This resolves the ambiguity in Python/Ruby where `any` and `find` are different functions.

### 7. Support multi-variable bindings for independence, nesting for dependence

```
forall x in S, y in T | P(x, y)            -- independent ranges
forall x in S | forall y in x.related | P  -- dependent range
forall x in S, y in S where x != y | P     -- guarded independence
```

The flat comma syntax signals "these ranges are independent." Nesting signals "the second range depends on the first." The `where` guard on a flat binding handles the "all pairs" pattern without nesting.
