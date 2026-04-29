# Relational Algebra for Evident

Evident programs manipulate sets of records — what relational algebra calls *relations*. Understanding the full vocabulary of relational operations clarifies which primitives Evident needs, which can be derived, and where the language design choices lie.

---

## What Is a Relation?

A **relation** `R` is a set of tuples, all sharing the same attribute schema. It is written `R : {A₁: T₁, A₂: T₂, ..., Aₙ: Tₙ}` where the Aᵢ are attribute names and Tᵢ are types. The set of attribute names is the **schema** of R.

Key properties:
- No duplicate tuples (it's a set, not a bag)
- No ordering on rows or columns
- Every tuple has the same attributes

This maps directly to Evident's claims-as-sets model: a named relation is a set of ground facts sharing a predicate shape.

---

## The Five Basic Operations (Codd, 1970)

E. F. Codd's original formulation gives five operations that together are relationally complete — any query expressible in first-order logic over relations can be expressed using only these five.

### 1. Selection (σ) — Filter Rows

**Mathematical notation:** `σ_P(R)`

Filter the rows of R to those satisfying predicate P.

**Type signature:** `(Relation[S], Predicate[S]) → Relation[S]`
The output schema is identical to the input schema.

**SQL equivalent:** `WHERE` clause

```sql
SELECT * FROM employees WHERE department = 'engineering'
```

is `σ_{department='engineering'}(employees)`.

**Constraint programming interpretation:** Selection is a constraint. `σ_P(R)` says "keep only records where P holds" — P is a constraint applied to each tuple. In Evident terms: `evident in_engineering(e) because { e.department == 'engineering' }` is selection applied to a set of employee records.

---

### 2. Projection (π) — Keep Certain Columns

**Mathematical notation:** `π_{A₁, A₂, ..., Aₖ}(R)`

Discard all attributes except the listed ones. Because the result is a set, duplicate rows after discarding attributes are merged into one.

**Type signature:** `(Relation[S], subset of attributes K ⊆ S) → Relation[K]`

**SQL equivalent:** The column list in `SELECT`

```sql
SELECT name, salary FROM employees
```

is `π_{name, salary}(employees)`.

**Constraint programming interpretation:** Projection is existential quantification over the dropped attributes. `π_{name}(employees)` is the set of all names `n` such that `∃ tuple t ∈ employees. t.name = n`. In Evident: projecting a record type to fewer fields corresponds to asking "does any record matching this partial shape exist?"

---

### 3. Cartesian Product (×) — All Combinations

**Mathematical notation:** `R × S`

Every tuple from R paired with every tuple from S. If R has m rows and S has n rows, the result has m×n rows. Attribute names must be disjoint (rename if necessary).

**Type signature:** `(Relation[A], Relation[B]) → Relation[A ∪ B]` (where A ∩ B = ∅)

**SQL equivalent:** `FROM A, B` (without a join condition), or `CROSS JOIN`

```sql
SELECT * FROM employees, departments
```

**Constraint programming interpretation:** Cartesian product is the *unconstrained combination* — what you get before adding any join condition. It's the basis for all joins. In constraint terms, it introduces two sets of variables that are initially independent; subsequent predicates (selection) constrain how they relate. This is precisely the generate-and-test structure that constraint propagation optimizes.

---

### 4. Union (∪) — Combine Two Relations

**Mathematical notation:** `R ∪ S`

All tuples from either R or S. R and S must have the same schema. Duplicate tuples (appearing in both) appear only once.

**Type signature:** `(Relation[S], Relation[S]) → Relation[S]`

**SQL equivalent:** `UNION` (deduplicating) vs. `UNION ALL` (bag union, which is not standard relational algebra)

```sql
SELECT name FROM current_employees
UNION
SELECT name FROM former_employees
```

**Constraint programming interpretation:** Union is disjunction. `R ∪ S` = "tuples that satisfy R's conditions OR S's conditions." In Evident: multiple `because` clauses for the same claim are union — any one derivation path suffices.

---

### 5. Difference (−) — Rows in One but Not the Other

**Mathematical notation:** `R − S`

Tuples in R that are not in S. R and S must have the same schema.

**Type signature:** `(Relation[S], Relation[S]) → Relation[S]`

**SQL equivalent:** `EXCEPT` (or `MINUS` in some dialects)

```sql
SELECT name FROM employees
EXCEPT
SELECT name FROM managers
```

**Constraint programming interpretation:** Difference is negation-as-failure — the tuples for which the second relation has no witness. This is the one operation that breaks monotonicity: adding rows to S can only *remove* rows from R − S. This connects to the open-world vs. closed-world assumption debate. In CLP and Datalog, negation is handled carefully (stratified negation, well-founded semantics) for exactly this reason.

---

## Derived Operations

These can all be expressed in terms of the five basic operations, but are common enough to deserve names.

### Natural Join (⋈)

**Mathematical notation:** `R ⋈ S`

Join R and S on all attributes they share. Equivalent to: take the Cartesian product, then select rows where shared attributes agree, then project to eliminate duplicates of shared attributes.

`R ⋈ S = π_{attrs(R) ∪ attrs(S)}(σ_{R.A₁=S.A₁ ∧ ... ∧ R.Aₖ=S.Aₖ}(R × S))`

where A₁...Aₖ are the shared attribute names.

**Type signature:** `(Relation[A], Relation[B]) → Relation[A ∪ B]`
(no duplication of shared attributes in the output schema)

**SQL equivalent:** `NATURAL JOIN` (rarely used explicitly; most joins are theta joins)

```sql
SELECT * FROM employees NATURAL JOIN departments
```

**Constraint programming interpretation:** Natural join is the key "combining" operation. It says: "find all pairs of records from R and S that agree on their shared fields." This is very close to unification — matching records on shared attributes is structural unification of those fields.

---

### Theta Join (⋈_θ)

**Mathematical notation:** `R ⋈_θ S`

Cartesian product filtered by an arbitrary predicate θ. The predicate can reference attributes from both R and S.

`R ⋈_θ S = σ_θ(R × S)`

**Type signature:** `(Relation[A], Relation[B], Predicate[A ∪ B]) → Relation[A ∪ B]`

**SQL equivalent:** `JOIN ... ON condition`

```sql
SELECT * FROM orders o JOIN shipments s ON o.id = s.order_id AND s.weight < 100
```

**Constraint programming interpretation:** Theta join is the general case of combining two sets of records with an arbitrary constraint between them. The constraint θ is a multi-variable predicate. This is essentially what Evident does whenever it joins two claim types: the rule body is a theta join condition.

---

### Equijoin

Special case of theta join where θ is a conjunction of equality conditions. The most common join in practice. When the equated attributes share a name, equijoin and natural join coincide.

---

### Left Outer Join (⟕), Right Outer Join (⟖), Full Outer Join (⟗)

**Mathematical notation:** `R ⟕ S`, `R ⟖ S`, `R ⟗ S`

Left outer join: all tuples from R, joined with matching tuples from S where they exist. If no match in S, the S attributes are filled with `NULL`.

`R ⟕ S = (R ⋈ S) ∪ ((R − π_{attrs(R)}(R ⋈ S)) × {null-tuple-of-S})`

**Type signature:** `(Relation[A], Relation[B]) → Relation[A ∪ B]` (with B attributes nullable)

**SQL equivalent:** `LEFT JOIN`, `RIGHT JOIN`, `FULL OUTER JOIN`

```sql
SELECT e.name, d.budget
FROM employees e LEFT JOIN departments d ON e.dept_id = d.id
```

**Constraint programming interpretation:** Outer joins introduce partiality — records that partially match but have unknown/missing values in some fields. This corresponds to allowing `NULL` or `UNKNOWN` in a constraint system, which complicates the semantics significantly. Most constraint solvers prefer totality; `NULL` semantics are a source of bugs in SQL.

---

### Intersection (∩)

**Mathematical notation:** `R ∩ S`

Tuples present in both R and S. Derivable: `R ∩ S = R − (R − S)`.

**Type signature:** `(Relation[S], Relation[S]) → Relation[S]`

**SQL equivalent:** `INTERSECT`

```sql
SELECT employee_id FROM project_alpha
INTERSECT
SELECT employee_id FROM project_beta
```

**Constraint programming interpretation:** Intersection is conjunction — tuples that simultaneously satisfy both relations' membership conditions. This is the most natural operation in a constraint system: adding constraints can only shrink the solution set.

---

### Division (÷) — "For All" Queries

**Mathematical notation:** `R ÷ S`

Given `R : {A, B}` and `S : {B}`, `R ÷ S` returns the set of A-values such that for *every* B-value in S, there is a matching tuple in R.

`R ÷ S = π_A(R) − π_A((π_A(R) × S) − R)`

**Type signature:** `(Relation[A ∪ B], Relation[B]) → Relation[A]`

**SQL equivalent:** No direct operator. Expressed as double negation:

```sql
-- "Employees who have worked on ALL projects"
SELECT e.id FROM employees e
WHERE NOT EXISTS (
    SELECT p.id FROM projects p
    WHERE NOT EXISTS (
        SELECT 1 FROM assignments a
        WHERE a.employee_id = e.id AND a.project_id = p.id
    )
)
```

**Constraint programming interpretation:** Division is universal quantification — the "for all" query. This is the hardest operation to express in SQL and maps naturally to `∀` in logic. In Evident: "claim A holds for every element of set S" is division. It requires negation-as-failure or a counting argument to compute.

---

### Rename (ρ)

**Mathematical notation:** `ρ_{A←B}(R)` or `ρ_{new_name}(R)`

Rename an attribute or the entire relation. Necessary for self-joins and to make attribute names compatible for union/difference.

**Type signature:** `(Relation[S]) → Relation[S']` where S' is S with renamed attributes

**SQL equivalent:** Column aliases (`AS`) and table aliases

```sql
SELECT e1.name AS manager, e2.name AS report
FROM employees e1, employees e2
WHERE e2.manager_id = e1.id
```

**Constraint programming interpretation:** Rename is purely structural — it has no logical content. It's a bookkeeping operation to resolve naming conflicts. In Evident's claim-based model, argument names in a predicate are positions, so renaming corresponds to reordering or relabeling predicate arguments.

---

### Aggregation (γ)

**Mathematical notation:** `γ_{G; F₁(A₁)→B₁, ..., Fₖ(Aₖ)→Bₖ}(R)`

Group R by attributes G, then apply aggregate functions F over each group.

**Type signature:** `(Relation[S]) → Relation[G ∪ {B₁, ..., Bₖ}]`

**SQL equivalent:** `GROUP BY` with aggregate functions

```sql
SELECT department, COUNT(*) AS headcount, AVG(salary) AS avg_salary
FROM employees
GROUP BY department
```

Standard aggregate functions:
- `COUNT(*)` — cardinality of group
- `SUM(A)` — sum of attribute A over group
- `MAX(A)`, `MIN(A)` — extrema
- `AVG(A)` — mean (= SUM/COUNT)

**Constraint programming interpretation:** Aggregation breaks the closure property (see below) in the strict sense — a count is not a tuple attribute in the same way a name is. More importantly, aggregation introduces *summaries* of sets, which require fully enumerating a group before computing the result. This is fundamentally non-declarative: you must know the complete extent of a group to compute its aggregate, which conflicts with open-world assumptions. In Evident: aggregation is the operation that most requires a closed-world assumption (CWA) over the relevant set.

---

## SQL to Relational Algebra Mapping

| SQL Clause | Relational Algebra Operation | Notes |
|---|---|---|
| `SELECT a, b, c` | Projection π_{a,b,c} | Eliminates columns; deduplicates in RA, not in SQL by default |
| `SELECT *` | No projection (identity) | All attributes pass through |
| `SELECT DISTINCT` | Projection π | SQL's `SELECT` without `DISTINCT` is bag semantics; `DISTINCT` gives set semantics |
| `FROM R` | Relation reference | Names a base relation |
| `FROM R, S` | Cartesian product R × S | Implicit cross join |
| `JOIN S ON condition` | Theta join R ⋈_θ S | The most common join form |
| `NATURAL JOIN S` | Natural join R ⋈ S | Joins on all shared attribute names |
| `LEFT JOIN S ON condition` | Left outer join R ⟕_θ S | NULLs for unmatched right-side rows |
| `RIGHT JOIN S ON condition` | Right outer join R ⟖_θ S | NULLs for unmatched left-side rows |
| `FULL OUTER JOIN S ON condition` | Full outer join R ⟗_θ S | NULLs on both sides for unmatched rows |
| `WHERE condition` | Selection σ_condition | Filters rows after FROM |
| `HAVING condition` | Selection σ_condition | Filters *after* aggregation; same operation, different timing |
| `GROUP BY g₁, g₂` | Aggregation γ_{g₁,g₂; ...} | Groups rows for aggregate functions |
| `COUNT(*)`, `SUM(a)`, etc. | Aggregate functions in γ | Applied per group |
| `UNION` | Union R ∪ S | Set union; deduplicates |
| `UNION ALL` | Bag union | Not standard RA; adds duplicates |
| `INTERSECT` | Intersection R ∩ S | Both relations must match |
| `EXCEPT` / `MINUS` | Difference R − S | Rows in first but not second |
| `AS alias` | Rename ρ | Renames relation or attribute |
| `ORDER BY` | Not relational algebra | Ordering is outside RA; relations are unordered sets |
| `LIMIT / OFFSET` | Not relational algebra | Pagination is outside RA |

### A Full SELECT as Relational Algebra

```sql
SELECT department, AVG(salary) AS avg_sal
FROM employees
WHERE hire_year >= 2020
GROUP BY department
HAVING AVG(salary) > 80000
ORDER BY avg_sal DESC
```

Relational algebra (bottom-up, inner to outer):

```
π_{department, avg_sal}(
  σ_{avg_sal > 80000}(
    γ_{department; AVG(salary)→avg_sal}(
      σ_{hire_year >= 2020}(
        employees
      )
    )
  )
)
```

The `ORDER BY` has no relational algebra equivalent — it is applied after RA evaluation and produces a *sequence*, not a *set*.

---

## The Closure Property

**Definition:** A family of operations is *closed* over a type if every operation in the family takes values of that type and produces a value of the same type.

Relational algebra is closed over relations: every operation takes one or two relations as input and produces a relation as output. This is what makes relational algebra *compositional*.

**Why closure matters:**
- You can pipe the output of one operation directly into another: `σ_P(π_A(R ⋈ S))` is always well-formed
- Queries can be nested arbitrarily
- Optimization is local: any sub-expression of a query is itself a valid query and can be independently optimized or reordered

**The type-theoretic view:** If `Relation[S]` is a type (parameterized by schema S), then the operations are:
```
σ   : (Relation[S], Predicate[S]) → Relation[S]
π   : (Relation[S]) → Relation[K]      where K ⊆ S
×   : (Relation[A], Relation[B]) → Relation[A ∪ B]
∪   : (Relation[S], Relation[S]) → Relation[S]
−   : (Relation[S], Relation[S]) → Relation[S]
⋈   : (Relation[A], Relation[B]) → Relation[A ∪ B]
ρ   : (Relation[S]) → Relation[S']
```

Each output is a relation. The schema may change (projection narrows it, product widens it) but the *kind* of thing — a relation — is preserved.

**Aggregation partially breaks closure:** `γ_{dept; COUNT(*)→n}(employees)` produces a relation `{dept: String, n: Int}` — still a relation, but with a computed attribute that has no direct original-column correspondent. The closure property is formally maintained, but the semantic type of attributes changes. Some treatments of extended relational algebra handle this carefully; others treat it pragmatically.

**For Evident:** The closure property is directly analogous to the claim algebra being closed: every operation on sets of records produces a set of records, which can be the input to further operations. This is what makes claim composition work.

---

## Extended Relational Algebra

Beyond the classical operations, modern SQL systems provide extensions that are not expressible in classical RA.

### Window Functions (OVER)

Window functions compute aggregate-like values *without collapsing rows*. Each row keeps its identity and gains a computed attribute derived from a "window" of nearby rows.

```sql
SELECT name, salary,
       RANK() OVER (PARTITION BY department ORDER BY salary DESC) AS rank,
       AVG(salary) OVER (PARTITION BY department) AS dept_avg
FROM employees
```

**Relational algebra extension:** `Ω_{partition; order; function→attr}(R)`

This is not expressible in classical RA without multiple passes. A window function is semantically: for each row r, apply a function to the group of rows sharing r's partition key (ordered by the order key), and attach the result to r.

**For Evident:** Window functions are a form of *relative* or *contextual* computation — a record's value depends on its relationship to other records in the same set. This requires global knowledge of the set's extent, similar to aggregation.

---

### Lateral Joins (LATERAL / CROSS APPLY)

A lateral join allows the right side of a join to be a *subquery that references columns from the left side*. This is like a dependent Cartesian product.

```sql
SELECT e.name, t.top_project
FROM employees e
CROSS JOIN LATERAL (
    SELECT project_name AS top_project
    FROM assignments a
    WHERE a.employee_id = e.id
    ORDER BY hours DESC
    LIMIT 1
) t
```

**Relational algebra extension:** The right operand is a *function* of the current left tuple, not a fixed relation. Written `R ×ˡ f(R)` where f maps a tuple to a relation.

This corresponds to **monadic bind** for the list/set monad: take each element of R, apply f to get a new relation, and flatten the results. Lateral join is essentially `flatMap` on relations.

**For Evident:** Lateral joins are the operation that makes Evident's rule-head/body structure natural — the "body" of a claim rule is evaluated *in the context of* the head's bindings. This is already the structure of Datalog rules.

---

### Recursive Queries (WITH RECURSIVE)

Standard RA has no recursion. Recursive CTEs allow a query to reference its own output.

```sql
WITH RECURSIVE reachable(from_id, to_id) AS (
    -- Base case
    SELECT from_id, to_id FROM edges
    UNION ALL
    -- Recursive step
    SELECT r.from_id, e.to_id
    FROM reachable r JOIN edges e ON r.to_id = e.from_id
)
SELECT * FROM reachable WHERE from_id = 1
```

**Relational algebra extension:** This computes the *least fixed point* of a monotone operator. If T is the operator `edges ∪ (reachable ⋈ edges)`, then `WITH RECURSIVE` computes the smallest relation R such that `T(R) = R`.

This is exactly Datalog's operational model (see below). `WITH RECURSIVE` was added to SQL precisely to capture transitive closure and other recursive queries that first-order RA cannot express.

**For Evident:** Recursive claims — like `reachable(a, b) because { edge(a, b) }` and `reachable(a, c) because { edge(a, b), reachable(b, c) }` — correspond exactly to `WITH RECURSIVE`. The Evident runtime must compute least fixed points to evaluate recursive rules.

---

## Operations by Frequency in Practice

Based on analysis of real-world SQL query logs and textbook studies of query patterns:

| Rank | Operation | SQL Form | Frequency | Notes |
|---|---|---|---|---|
| 1 | Selection (σ) | `WHERE` | Ubiquitous | Almost every non-trivial query |
| 2 | Projection (π) | `SELECT col1, col2` | Ubiquitous | Column subsetting is universal |
| 3 | Equijoin / Theta join (⋈_θ) | `JOIN ... ON` | Very high | The workhorse of multi-table queries |
| 4 | Aggregation (γ) | `GROUP BY` + `COUNT`/`SUM`/etc. | High | Analytics and reporting |
| 5 | Left outer join (⟕) | `LEFT JOIN` | High | Preserving "unpaired" rows |
| 6 | Union (∪) | `UNION` | Medium | Combining result sets |
| 7 | Intersection (∩) | `INTERSECT` | Low | Often expressed as join instead |
| 8 | Difference (−) | `EXCEPT` | Low | Often expressed as `NOT EXISTS` |
| 9 | Window functions (Ω) | `OVER (PARTITION BY ...)` | Medium-high | Analytics; growing in use |
| 10 | Recursive queries | `WITH RECURSIVE` | Low-medium | Graph/hierarchy traversal |
| 11 | Division (÷) | (double negation) | Rare | "For all" queries; hard to write |
| 12 | Cartesian product (×) | `CROSS JOIN` | Rare | Almost always unintentional |
| 13 | Natural join (⋈) | `NATURAL JOIN` | Very rare | Fragile; avoided in practice |

**Key practical observation:** The five basic operations are not equal in frequency. Selection and projection are universal. Cartesian product in its raw form is almost never intentional — its role in the theory is to build toward joins. Division is theoretically important (universal quantification) but rarely appears in practice because it's hard to write in SQL.

---

## Datalog vs. Relational Algebra

Datalog is a logic programming language that sits between relational algebra and Prolog. Understanding its relationship to RA is important for Evident's design.

### How Datalog Rules Map to RA

A Datalog rule:
```datalog
reachable(X, Z) :- edge(X, Y), reachable(Y, Z).
```

corresponds to the relational algebra expression:
```
π_{X,Z}( edge ⋈_{edge.Y = reachable.Y} reachable )
```

That is: a Datalog rule body is a natural join of the body predicates, followed by a projection to the head's variables. Conjunction of body literals is join; variable sharing between literals is the join condition; the head is projection.

More precisely:

| Datalog construct | Relational algebra equivalent |
|---|---|
| Base fact (EDB predicate) | Base relation |
| Derived predicate (IDB) | Defined relation (view) |
| Rule body literal `p(X, Y)` | Reference to relation p with attributes X, Y |
| Variable shared between literals | Join condition (equijoin on shared variable) |
| Multiple body literals | Natural join of the literals |
| Rule head `q(X, Z)` | Projection to X, Z |
| Multiple rules for same head | Union of the rule bodies |
| Negated literal `¬p(X)` | Difference (R − π(p)) |

### What Datalog Adds Over Relational Algebra

**Recursion (least fixed point):** The defining addition. Datalog can express transitive closure; classical RA cannot. A set of Datalog rules is evaluated by computing the least fixed point of the associated RA operator, iterating until no new tuples are derived.

```datalog
-- Transitive closure: impossible in classical RA
reachable(X, Y) :- edge(X, Y).
reachable(X, Z) :- edge(X, Y), reachable(Y, Z).
```

**Uniform rule syntax:** Every RA expression can be written as a set of Datalog rules, but the mapping goes both ways naturally. Datalog's rule notation makes the query structure transparent in a way SQL subqueries obscure.

**Stratified negation:** Datalog with negation (Datalog¬) allows negated body literals, computed in a bottom-up layered (stratified) order that ensures consistent semantics. This corresponds to the difference operation applied in safe order.

**What Datalog lacks compared to SQL:**
- Aggregation (not in pure Datalog; Datalog± and Datalog^agg are extensions)
- Arithmetic on aggregate results
- `ORDER BY`, `LIMIT`
- Outer joins
- Window functions

### The Seminal Connection

The semi-naïve bottom-up evaluation of Datalog is an efficient implementation of the least-fixed-point operator for a set of RA expressions. The datalog → RA translation is:

Given rules for predicate P:
1. Translate each rule body to an RA expression (join + project)
2. Union all rule bodies for P
3. Compute the least fixed point by iterating: `P_{i+1} = P_i ∪ T(P_i)` until `P_{i+1} = P_i`

Semi-naïve evaluation optimizes step 3 by computing only *new* tuples at each iteration (the "delta" relation), avoiding redundant work.

### Evident Is Essentially Datalog with Constraints

Evident's claim structure corresponds to Datalog:
- A claim predicate is a Datalog predicate
- `evident q(X) because { p₁(X,Y), p₂(Y,Z) }` is the Datalog rule `q(X) :- p₁(X,Y), p₂(Y,Z)`
- Multiple `because` clauses for the same claim are multiple rules (union of bodies)
- Recursive claims are Datalog recursion (least fixed point)
- The constraint solver handling arithmetic/SMT literals corresponds to built-in predicates in Datalog that are dispatched to an external solver

The key difference: where Datalog uses Herbrand semantics (only symbolic ground terms), Evident's constraint solver handles rich theories (linear arithmetic, algebraic data types, etc.) without enumerating their ground instances. This is the move from Datalog to Datalog with constraints (CLP/CHC — constrained Horn clauses).

---

## Summary: Operations Evident Should Support

Working from the above, here is a priority ordering for Evident's relational primitives:

**Essential (must have):**
- Selection (σ): constraint on records — the core operation
- Natural join / equijoin (⋈): combining claim types on shared attributes
- Projection (π): existential quantification, dropping fields
- Union (∪): multiple derivation paths (`because` alternatives)
- Recursion (fixed point): recursive claims

**Important (high value):**
- Difference (−): negation-as-failure, closed-world queries
- Aggregation (γ): COUNT, SUM — requires closed-world assumption
- Rename (ρ): structural bookkeeping for self-joins and name conflicts

**Useful extensions:**
- Outer join (⟕): partial matches with defaults/nulls
- Window functions (Ω): contextual/relative computation
- Lateral join: dependent combinations (naturally present in rule structure)

**Lower priority:**
- Division (÷): expressible via double negation; "for all" claims
- Cartesian product (×): subsumed by join; appears in theory, rarely wanted raw
- Natural join (⋈): convenient but fragile; explicit equijoin is safer
