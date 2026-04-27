# Alternative Notations for Set Theory

> Research for Evident language design — surveying how set-theoretic concepts have been expressed in ways other than the standard mathematical notation (∈, ⊆, ∪, ∩, \, ×, 𝒫, {x | P(x)}, ∀, ∃).

---

## 1. Programming Language Notations

### 1.1 Alloy

Alloy is a lightweight object modeling language rooted in relational logic and set theory. Its notation deliberately departs from standard math:

| Concept | Standard Math | Alloy |
|---------|--------------|-------|
| Union | A ∪ B | `A + B` |
| Intersection | A ∩ B | `A & B` |
| Difference | A \ B | `A - B` |
| Membership | x ∈ A | `x in A` |
| Subset | A ⊆ B | `A in B` (set inclusion is just subset) |
| Empty set | ∅ | `none` |
| Universal set | U | `univ` |
| Cartesian product | A × B | `A -> B` |
| Relation join | R(S) | `S.R` (dot operator = relational join) |
| Transitive closure | R⁺ | `^R` |
| Reflexive-transitive closure | R* | `*R` |
| Relation reversal | R⁻¹ | `~R` |
| Domain restriction | A ◁ R | `A <: R` |
| Range restriction | R ▷ A | `R :> A` |
| Cardinality | |A| | `#A` |
| Relational override | R ⊕ S | `R ++ S` |

**Set comprehension:**
```alloy
{x: Set1 | expr[x]}
{x: Set1, y: Set2 | expr[x,y]}
```

**Quantifiers:**
```alloy
all f : File | restore[f] implies once delete[f]   -- universal (∀)
some f : File | delete[f]                           -- existential (∃)
no Trash                                            -- emptiness (¬∃)
lone x                                              -- at most one (∃≤1)
one x                                               -- exactly one (∃!)
```

**Signatures (typed set declarations):**
```alloy
sig File {}
sig Trash in File {}   -- Trash is a subset of File
```

**Key insight:** Alloy uses `+` for union (not `|`), uses `in` for membership (not `∈`), and its dot operator `.` is relational join — the primary way to navigate relations. Everything is a set; scalars are singleton sets.

---

### 1.2 TLA+

TLA+ uses notation close to standard mathematics but with some distinctive choices:

| Concept | Standard Math | TLA+ |
|---------|--------------|------|
| Union | A ∪ B | `A ∪ B` (unicode) or `A \union B` |
| Intersection | A ∩ B | `A ∩ B` or `A \intersect B` |
| Difference | A \ B | `A \ B` |
| Membership | x ∈ A | `x ∈ A` or `x \in A` |
| Non-membership | x ∉ A | `x \notin A` |
| Subset | A ⊆ B | `A ⊆ B` or `A \subseteq B` |
| Power set | 𝒫(A) | `SUBSET A` |
| Big union | ⋃S | `UNION S` |
| Set comprehension | {x ∈ S | P(x)} | `{x ∈ S : P(x)}` (colon not pipe) |
| Replacement | {f(x) | x ∈ S} | `{F(x) : x ∈ S}` |
| Interval | {n ∈ ℤ | a ≤ n ≤ b} | `a .. b` |
| Function set | Bᴬ | `[A → B]` |
| Existential choice | ε x. P(x) | `CHOOSE x : P(x)` |
| Universal | ∀x ∈ S: P | `∀x ∈ S : P(x)` |
| Existential | ∃x ∈ S: P | `∃x ∈ S : P(x)` |

**Key insight:** TLA+ uses `:` instead of `|` in comprehensions. `SUBSET` and `UNION` are uppercase keywords. The `..` interval notation is built-in. `CHOOSE` is the Hilbert epsilon operator.

---

### 1.3 Z Notation

Z is a formal specification language based on typed set theory and first-order predicate calculus. It uses schemas (named collections of typed variables + predicates) as its organizing structure.

| Concept | Z Notation |
|---------|-----------|
| Set comprehension | `{x : T | pred(x) • expr(x)}` |
| Type of all subsets | `ℙ T` (power set type, written P) |
| Non-empty subsets | `ℙ₁ T` |
| Finite subsets | `𝔽 T` |
| Set size | `#S` |
| Interval | `a .. b` |
| Function (total) | `A → B` |
| Function (partial) | `A ⇸ B` |
| Function (injection) | `A ↣ B` |
| Function (bijection) | `A ⤖ B` |

**Schema notation:**
```
┌─ Person ─────────────────
│ name : String
│ age : ℕ
├──────────────────────────
│ age > 0
└──────────────────────────
```

**Key insight:** Z uses the `•` (bullet) to separate the binding from the expression in comprehensions: `{x : T | P(x) • f(x)}` reads "the set of f(x) for all x of type T where P(x)". Z has rich arrow types for functions (→, ⇸, ↣, ↠, ⤖ etc.).

---

### 1.4 B Method / Event-B

B method uses similar notation to Z, focusing on refinement and implementation. Key differences:

| Concept | B Method Notation |
|---------|-------------------|
| Power set | `POW(S)` |
| Non-empty subsets | `POW1(S)` |
| Finite subsets | `FIN(S)` |
| Set comprehension | `{x | P(x)}` |
| Lambda (function) | `%(x).(P(x) | E(x))` |
| Generalized union | `UNION(x).(P(x) | f(x))` |
| Generalized intersection | `INTER(x).(P(x) | f(x))` |
| Function application | `f(x)` |
| Substitution | `x := E` |

**Key insight:** B uses all-caps keywords (`POW`, `FIN`, `UNION`, `INTER`) for set-level operators. The lambda/comprehension pattern `%(x).(predicate | expression)` is a notable alternative.

---

### 1.5 OCL (Object Constraint Language)

OCL uses method-call (dot/arrow) notation on collections rather than set-theoretic symbols. Collections are typed: `Set`, `Bag`, `Sequence`, `OrderedSet`.

| Concept | Standard Math | OCL |
|---------|--------------|-----|
| Filter | {x ∈ S | P(x)} | `S->select(x \| P(x))` |
| Reject (complement filter) | {x ∈ S | ¬P(x)} | `S->reject(x \| P(x))` |
| Map | {f(x) | x ∈ S} | `S->collect(x \| f(x))` |
| Universal | ∀x ∈ S: P(x) | `S->forAll(x \| P(x))` |
| Existential | ∃x ∈ S: P(x) | `S->exists(x \| P(x))` |
| Membership | x ∈ S | `S->includes(x)` |
| Non-membership | x ∉ S | `S->excludes(x)` |
| Cardinality | |S| | `S->size()` |
| Non-empty | S ≠ ∅ | `S->notEmpty()` |
| Empty | S = ∅ | `S->isEmpty()` |
| Union | A ∪ B | `A->union(B)` |
| Intersection | A ∩ B | `A->intersection(B)` |
| Is subset | A ⊆ B | `A->includesAll(B)` |

**Examples:**
```ocl
self.parents->forAll(p | p.age > self.age)
self.cars->exists(c | Calendar.YEAR - c.constructionYear < self.age)
{1,2,3,4,5}->select(each | each > 3)    -- = {4,5}
{'a','bb','ccc'}->collect(each | each.toUpper())
```

**Key insight:** OCL uses `->` to call collection operations as methods, using `|` inside for the variable separator. This is a fluent/object-oriented syntax for set theory.

---

### 1.6 SPARQL

SPARQL queries RDF graphs using graph pattern matching. Its set-theoretic constructs appear in English-keyword form:

```sparql
SELECT ?person ?name
WHERE {
  ?person rdf:type foaf:Person .
  ?person foaf:name ?name .
  FILTER (?age > 18)
  OPTIONAL { ?person foaf:email ?email }
}

-- Union of patterns:
{ ?x a :Dog } UNION { ?x a :Cat }

-- Negation:
FILTER NOT EXISTS { ?person :hasCriminalRecord ?r }
FILTER EXISTS { ?person :hasJob ?j }
MINUS { ?person :banned true }
```

**Key insight:** SPARQL uses SQL-style English keywords (`SELECT`, `WHERE`, `FILTER`, `UNION`, `OPTIONAL`, `MINUS`) to express set operations on RDF triple patterns. `?var` denotes logic variables. Pattern matching is fundamentally a join.

---

## 2. Diagrammatic and Visual Set Theory

### 2.1 Venn Diagrams (1880)

John Venn's notation uses overlapping circles where:
- Circle = set
- Overlap region = intersection
- Non-overlapping part = difference
- Shaded region = empty set
- All circles drawn (even if relationship doesn't exist)

Venn diagrams must show **all 2ⁿ possible** regions for n sets, even impossible/empty ones.

### 2.2 Euler Diagrams (1768)

Leonhard Euler's earlier notation is more flexible:
- Curves (not necessarily circles) represent sets
- Contained curve = subset
- Overlapping curves = intersection exists
- Separate curves = disjoint sets
- **Empty regions simply absent** (not shown)

Euler diagrams encode actual relationships, not all possible ones — this makes them more readable for real data but less systematic.

**Topological encodings:**
- Enclosure → subset (⊆)
- Overlap → non-empty intersection
- Separation → disjoint sets (∩ = ∅)

### 2.3 Spider Diagrams

An extension of Euler diagrams that adds "spiders" (connected graphs) to represent existential and universal statements about set elements. A spider touching multiple regions represents an element in their union.

---

## 3. Alternative Symbolic Notations

### 3.1 Words Instead of Symbols

Several approaches use English words:

| Symbol | Alternatives |
|--------|-------------|
| ∈ | `in`, `member`, `belongs to`, `is a`, `is an element of` |
| ∉ | `not in`, `nin`, `not member of` |
| ⊆ | `subset of`, `contained in`, `subseteq` |
| ⊂ | `proper subset of`, `strictly contained in` |
| ∪ | `union`, `or` |
| ∩ | `intersect`, `and`, `intersection` |
| \ | `minus`, `without`, `except`, `difference` |
| ∅ | `empty`, `{}`, `none`, `nil` |
| ∀ | `all`, `every`, `forall`, `for all` |
| ∃ | `some`, `exists`, `there exists` |
| ¬∃ | `no`, `none` |
| ∃! | `one`, `exactly one`, `unique` |
| ∃≤1 | `lone`, `at most one` |

### 3.2 Alloy's Quantifier Keywords

Alloy extended the standard two quantifiers (∀, ∃) to five, making code more expressive:

- `all` — for all (∀)
- `some` — there exists (∃)
- `no` — for none (¬∃)
- `one` — there exists exactly one (∃!)
- `lone` — there exists at most one (∃≤1)

### 3.3 Colon vs. Pipe in Comprehensions

The set comprehension `{x | P(x)}` uses `|` (pipe) in most notations, but:
- TLA+ and Z use `:` — `{x ∈ S : P(x)}`
- Z adds `•` for the output expression — `{x : T | P(x) • f(x)}`
- Some use `where` — `{x in S where P(x)}`
- Some use `such that` — `{x in S such that P(x)}`

### 3.4 Postfix/Method Style

Instead of `P(x)` or `x ∈ P`, the OCL/OOP style uses:
```
collection.filter(predicate)
collection.map(function)
collection.exists(predicate)
collection.forAll(predicate)
```

### 3.5 APL / J Array Language Notation

APL uses special Unicode glyphs. Set-related functions include:

| Operation | APL Glyph | Name |
|-----------|-----------|------|
| Union | `A ∪ B` | Union (dyadic) |
| Intersection | `A ∩ B` | Intersection (dyadic) |
| Unique/Nub | `∪A` | Unique (monadic — removes duplicates) |
| Membership | `A ∊ B` | Member of |
| Index | `A⍳B` | Index of |
| Without | `A~B` | Without (set difference for arrays) |

APL's philosophy: terse, symbol-heavy, compositional. Maximum expressiveness per character, minimum readability for non-practitioners.

---

## 4. Type-Theoretic Notations

### 4.1 Martin-Löf Type Theory (MLTT)

MLTT corresponds set theory concepts to types:

| Set Theory | Type Theory |
|-----------|------------|
| Set A | Type A |
| x ∈ A | x : A |
| ∅ | ⊥ (bottom/empty type) |
| {∗} (singleton) | ⊤ (unit type) |
| A ∪ B | A + B (sum/coproduct type) |
| A ∩ B | (context-dependent) |
| A × B | A × B (product type) |
| A → B | A → B (function type) |
| {x ∈ A | P(x)} | Σ(x : A), P(x) (sigma type) |
| ∀x ∈ A, P(x) | Π(x : A), P(x) (pi type) |
| ∃x ∈ A, P(x) | Σ(x : A), P(x) (sigma type again) |

**Key insight:** In MLTT, membership is **typing judgment** (`x : A`), not a proposition. The sigma type `Σ(x : A), P(x)` serves double duty as both {x ∈ A | P(x)} (subset comprehension) and ∃x ∈ A, P(x) (existential).

**Judgment forms:**
```
A : Type          -- A is a type
x : A             -- x is an element of type A (membership)
a = b : A         -- a and b are equal elements of A
A = B : Type      -- A and B are the same type
```

### 4.2 Lean 4 Set Notation

Lean 4 implements sets as predicates (`Set α = α → Prop`):

```lean
x ∈ A            -- membership (\in)
x ∉ A            -- non-membership (\notin)
A ⊆ B            -- subset (\subeq)
A ∪ B            -- union (\un)
A ∩ B            -- intersection (\i)
A \ B            -- difference (\\)
Aᶜ               -- complement (\^c)
∅                -- empty set (\empty)
univ             -- universal set
{x : ℕ | x < 10} -- set comprehension
⋃ i, A i         -- indexed union (\Un)
⋂ i, A i         -- indexed intersection (\I)
```

**Key insight:** `A ⊆ B` is definitionally equal to `∀ x, x ∈ A → x ∈ B`. `x ∈ A ∩ B` is definitionally `x ∈ A ∧ x ∈ B`. Sets are predicates; set operations are logical operations.

### 4.3 Agda

Agda uses a similar but more syntactically flexible notation, supporting Unicode operators and mixfix syntax:

```agda
x ∈ A
A ⊆ B
A ∪ B
A ∩ B
∅
(a : A) → B a    -- Pi type (function, universal)
Σ A B            -- Sigma type (existential, subset)
```

### 4.4 Curry-Howard Correspondence

The key correspondence for Evident:

| Logic/Sets | Types/Programs |
|-----------|---------------|
| Proposition P | Type P |
| Proof of P | Term of type P |
| P is true | P is **inhabited** (has a term) |
| P is false | P is **uninhabited** (no term exists) |
| P ∧ Q | P × Q (product) |
| P ∨ Q | P + Q (sum) |
| P → Q | P → Q (function) |
| ¬P | P → ⊥ |
| ∀x: P(x) | Π(x), P(x) |
| ∃x: P(x) | Σ(x), P(x) |

---

## 5. Database / Relational Algebra Notations

### 5.1 Classical Relational Algebra

The standard symbols (rarely used directly in code):

| Operation | Symbol | Meaning |
|-----------|--------|---------|
| Selection | σ_φ(R) | Rows where φ holds |
| Projection | π_{a,b}(R) | Keep only columns a, b |
| Rename | ρ_{new/old}(R) | Rename attribute |
| Natural join | R ⋈ S | Join on shared attributes |
| Theta join | R ⋈_θ S | Join with condition θ |
| Left outer join | R ⟕ S | Keep unmatched left rows |
| Right outer join | R ⟖ S | Keep unmatched right rows |
| Full outer join | R ⟗ S | Keep all unmatched rows |
| Aggregation | G_g(f(A))(R) | Group-by with aggregate |
| Union | R ∪ S | Combined rows |
| Difference | R \ S | Rows in R but not S |
| Intersection | R ∩ S | Common rows |
| Cartesian product | R × S | All pairs |

### 5.2 SQL Notation

SQL maps relational algebra to English keywords:

```sql
-- Selection (σ):
SELECT * FROM employees WHERE salary > 50000

-- Projection (π):
SELECT name, department FROM employees

-- Join (⋈):
SELECT e.name, d.name
FROM employees e JOIN departments d ON e.dept_id = d.id

-- Set union:
SELECT name FROM employees
UNION
SELECT name FROM contractors

-- Set difference:
SELECT name FROM employees
EXCEPT
SELECT name FROM contractors

-- Set intersection:
SELECT name FROM employees
INTERSECT
SELECT name FROM contractors

-- Aggregation:
SELECT dept, COUNT(*), AVG(salary)
FROM employees
GROUP BY dept
HAVING COUNT(*) > 5
```

**Key insight:** SQL uses `WHERE` for selection (filter), `SELECT` for projection (map), `JOIN` for relational join, `UNION`/`EXCEPT`/`INTERSECT` for set operations. The from-where-select order is backwards from mathematical set-builder notation but reads like English.

### 5.3 LINQ (C#) — from-where-select

LINQ reverses SQL's order to match data flow (source → filter → project):

```csharp
// Standard query syntax (SQL-like but in source order):
from x in collection
where x.Age > 18
select x.Name

// Method chain syntax:
collection
  .Where(x => x.Age > 18)
  .Select(x => x.Name)

// Set operations:
a.Union(b)
a.Intersect(b)
a.Except(b)
a.Contains(x)
a.Any(pred)       // ∃
a.All(pred)       // ∀
a.Count()
a.First()
a.GroupBy(key)
```

### 5.4 Clojure's clojure.set

Clojure expresses set operations as named functions:

```clojure
(clojure.set/union #{1 2 3} #{2 3 4})         ;; {1 2 3 4}
(clojure.set/intersection #{1 2 3} #{2 3 4})   ;; {2 3}
(clojure.set/difference #{1 2 3} #{2 3 4})     ;; {1}
(clojure.set/subset? #{1 2} #{1 2 3})          ;; true
(contains? #{1 2 3} 2)                         ;; membership check

;; Relational operations on sets of maps:
(clojure.set/select #(> (:age %) 18) people)   ;; filter rows
(clojure.set/project people [:name :age])       ;; project columns
(clojure.set/rename people {:name :full-name})  ;; rename attributes
(clojure.set/join employees departments)        ;; natural join

;; Sets as predicates:
(#{:red :green :blue} :red)    ;; returns :red (truthy)
(#{:red :green :blue} :black)  ;; returns nil (falsy)
```

---

## 6. Programming DSLs and Comprehension Notations

### 6.1 Comprehension Syntax Survey

The mathematical `{f(x) | x ∈ S, P(x)}` becomes:

| Language | Syntax |
|----------|--------|
| Math | `{2x | x ∈ S, x² > 3}` |
| Python | `{2*x for x in S if x**2 > 3}` |
| Haskell | `[2*x \| x <- S, x^2 > 3]` |
| Scala | `for (x <- S if x*x > 3) yield 2*x` |
| Erlang | `[2*X \|\| X <- S, X*X > 3]` |
| F# | `seq { for x in S do if x*x > 3 then yield 2*x }` |
| Clojure | `(for [x S :when (> (* x x) 3)] (* 2 x))` |
| C# LINQ | `from x in S where x*x > 3 select 2*x` |
| Julia | `[2x for x in S if x^2 > 3]` |
| SETL | `{2*m : m in S | m*m > 3}` |
| TLA+ | `{2*x : x ∈ S, x^2 > 3}` (filtering via inline predicate) |
| Alloy | `{x : S \| x.val > 3}` |

**Key patterns:**
- `for x in S` vs `x <- S` vs `x ∈ S` — variable binding
- `if P(x)` vs `where P(x)` vs `| P(x)` vs `:when P(x)` — filtering
- `yield f(x)` vs `select f(x)` vs `collect f(x)` — projection
- `{...}` vs `[...]` — set vs list result

### 6.2 SETL (SET Language, 1960s Schwartz)

SETL was the first programming language directly based on set theory. It influenced Python's design.

```setl
-- Set comprehension:
{x * x : x in {1..10} | x mod 2 = 0}  -- squares of even numbers 1-10

-- Quantifiers:
forall x in S | P(x)                   -- ∀
exists x in S | P(x)                   -- ∃

-- Set operations (use English words or operators):
S1 + S2      -- union
S1 * S2      -- intersection  
S1 - S2      -- difference
x in S       -- membership
#S           -- cardinality

-- Tuple (ordered):
[a, b, c]

-- Range:
{1..10}
[1..N]

-- Map (set of pairs):
{[k, v] : [k, v] in M | P(k, v)}
```

**SETL example — Sieve of Eratosthenes:**
```setl
[n in [2..N] | forall m in {2..n-1} | n mod m > 0]
```

**Key insight:** SETL uses `in` for membership, `:` to separate output from domain in comprehensions, `|` for the filter predicate, `#` for cardinality, `+`/`*`/`-` for set operations. Range literals use `{a..b}` or `[a..b]`.

### 6.3 Answer Set Programming (ASP / Clingo)

ASP expresses sets through logical rules with stable model semantics:

```prolog
% Facts (elements of a relation):
person(alice). person(bob). person(carol).
age(alice, 30). age(bob, 17). age(carol, 25).

% Rules (derived sets):
adult(X) :- person(X), age(X, A), A >= 18.

% Choice rules (enumerate subsets):
{ selected(X) : person(X) } = 3.   -- choose exactly 3 people

% Aggregates:
:- #count { X : adult(X) } < 2.    -- at least 2 adults required

% Conditional literals:
:- not adult(X), selected(X).       -- selected people must be adults
```

**Key insight:** ASP uses Prolog-like rules but with set semantics. `{ member(X) : domain(X) }` is a choice rule generating subsets. `#count`, `#sum`, `#min`, `#max` are aggregate operators.

### 6.4 Prolog Set Predicates

Prolog doesn't have native sets but has meta-predicates for collecting solutions:

```prolog
findall(Template, Goal, List)    -- collect all as list (with duplicates)
bagof(Template, Goal, List)      -- like findall but fails if none
setof(Template, Goal, Set)       -- like bagof but sorted, unique

% Examples:
setof(X, age(X, _), People)            -- set of all people
setof(X, Y^(age(X, Y), Y >= 18), Adults)  -- X^Y means ∃Y

% Membership:
member(X, [H|T]) :- X = H ; member(X, T).
memberchk(X, List)   -- deterministic membership check
```

**Key insight:** Prolog uses `^` for existential quantification in `bagof`/`setof`. Reading `Y^Goal` as "there exists Y such that Goal" is an interesting notation.

### 6.5 Datalog

Datalog is a subset of Prolog used as a query/constraint language:

```datalog
% Facts:
parent(tom, bob).
parent(bob, ann).

% Rules (sets defined by derivation):
ancestor(X, Y) :- parent(X, Y).
ancestor(X, Y) :- parent(X, Z), ancestor(Z, Y).

% Queries:
?- ancestor(tom, Who).   -- find all X where ancestor(tom, X)
```

**Key insight:** Datalog has no explicit set syntax — sets emerge from the semantics of rules. Everything is a relation. The comma `,` in rule bodies is conjunction (and); semicolons `;` are disjunction (or).

---

## 7. Natural Language Approaches

### 7.1 Attempto Controlled English (ACE)

ACE is a CNL that translates automatically to first-order logic:

```
Every employee is a person.                    -- ∀x: employee(x) → person(x)
Some employee earns more than 100000.          -- ∃x: employee(x) ∧ earns(x, 100000)
No manager reports to a contractor.             -- ¬∃x,y: manager(x) ∧ contractor(y) ∧ reports(x,y)
Each department has at least 3 employees.      -- ∀d: dept(d) → |{e: emp(e) ∧ in(e,d)}| ≥ 3
```

**Key insight:** ACE uses "every" for ∀, "some" for ∃, "no" for ¬∃, "each" for ∀, avoiding all symbols.

### 7.2 ForTheL / SAD

ForTheL (Formula Theory Language) is used in the SAD proof assistant for near-mathematical-English:

```
Let A be a set. Let x be an element of A.
Define B = { x in A | x is prime }.
For all x in A there exists y in B such that y < x.
```

### 7.3 Near-English Patterns in Code

Many systems use near-English patterns that avoid symbols:

```
-- Python-style:
x in S                        -- membership
all(P(x) for x in S)         -- universal
any(P(x) for x in S)         -- existential
{x for x in S if P(x)}       -- comprehension

-- SQL-style English:
WHERE x IS NOT NULL
HAVING COUNT(*) > 3
WHERE x BETWEEN 1 AND 10
WHERE x IN (1, 2, 3)
WHERE x NOT IN (SELECT ...)
```

---

## 8. Interval and Range Notations

Different ways programming languages express ranges:

| Notation | Meaning | Used in |
|----------|---------|---------|
| `a..b` | [a, b] inclusive | TLA+, Z, SETL, Ruby, Kotlin, Swift |
| `a...b` | [a, b) exclusive upper | Ruby (`...` is exclusive), Swift |
| `a..=b` | [a, b] inclusive | Rust |
| `a..b` | [a, b) exclusive | Rust (without `=`) |
| `{a..b}` | {a, a+1, ..., b} | SETL, some shells |
| `[a..b]` | [a, a+1, ..., b] | Haskell, Pascal |
| `range(a, b)` | [a, b) half-open | Python |
| `[a, b]` | closed interval | Mathematics |
| `(a, b)` | open interval | Mathematics |
| `[a, b)` | half-open | Mathematics |
| `a:b` | [a, b-1] or stride | MATLAB, NumPy |
| `a:step:b` | arithmetic sequence | MATLAB |
| `a to b` | [a, b] inclusive | English-like |
| `a until b` | [a, b) exclusive | Scala, Kotlin |
| `a upto b` | [a, b] inclusive | Some DSLs |

**Key insight:** The mathematical convention `[a,b]` vs `(a,b)` uses bracket type to indicate inclusion, but this conflicts with tuple notation `(a,b)`. Most programming languages use `..` or `...` with different conventions for inclusion/exclusion.

---

## 9. Arrow and Path Notations

### 9.1 Category Theory Arrows

Category theory uses arrows for morphisms (structure-preserving maps):

```
f : A → B         -- function/morphism from A to B
g ∘ f             -- composition (apply f then g)
id_A              -- identity morphism on A
A ≅ B             -- isomorphism
A ↔ B             -- bijection
f ↓ g             -- comma category / comma object
A → B → C         -- diagram/path
```

Commutative diagrams express equalities by saying paths from A to B compose to equal morphisms.

### 9.2 Alloy's Dot Operator as Path Navigation

Alloy's `.` operator computes relational joins, which can be read as path traversal:

```alloy
-- If 'parent' is a relation Person -> Person:
alice.parent        -- the set of alice's parents
alice.parent.parent -- the set of alice's grandparents
^parent             -- transitive closure (all ancestors)
*parent             -- reflexive transitive closure

-- If 'owns' is Person -> Car:
Person.owns         -- all cars owned by any person
alice.owns          -- cars owned by alice
```

### 9.3 Arrow Notation in Type Theory

In dependent type theory, arrows have layered meanings:
- `A → B` — function type (non-dependent)
- `(x : A) → B(x)` — dependent function type (Pi type)
- `∀ (x : A), B(x)` — same thing in logical notation
- `Σ (x : A), B(x)` — dependent pair type (Sigma type)

### 9.4 Graph/Relation Notation in Datalog/SPARQL

```sparql
-- Path expressions in SPARQL 1.1 Property Paths:
?x foaf:knows ?y           -- direct edge
?x foaf:knows+ ?y          -- transitive closure (one or more)
?x foaf:knows* ?y          -- reflexive transitive closure (zero or more)
?x foaf:knows? ?y          -- optional (zero or one)
?x foaf:knows|foaf:trusts ?y  -- union of relations
?x !foaf:knows ?y          -- negation (any property except knows)
```

**Key insight:** SPARQL 1.1 added XPath-inspired property paths using `+`, `*`, `?`, `|`, `!` — the same symbols as regular expressions — to express transitive and alternative relationships.

### 9.5 Kleene Algebra / Regular Expression Notation

Regular expression algebra reuses set notation for *languages* (sets of strings):

| Operation | Symbol | Set Theory Analog |
|-----------|--------|------------------|
| Concatenation | `ab` or `a·b` | (not standard in set theory) |
| Union/alternation | `a\|b` or `a+b` | A ∪ B |
| Kleene star | `a*` | Σ* (closure) |
| Positive closure | `a+` | Σ⁺ (one or more) |
| Optional | `a?` | ε ∪ a |
| Complement | `¬a` or `[^a]` | Aᶜ |
| Intersection | `a&b` | A ∩ B |

The `*` (star) and `+` (closure) operators are compact notations for infinite unions.

---

## 10. Historical Alternatives

### 10.1 Cantor's Original Notation (1874–1895)

Georg Cantor introduced:
- Braces `{a, b, c}` for explicit sets (1878)
- **Mächtigkeit** (Mächtigkeit = cardinality), written with overlines and bars
- Ordinal numbers using Greek letters (ω, ω², etc.)
- **Aleph notation** (ℵ₀, ℵ₁) for infinite cardinals
- The notation **M = {m}** for "M is the set whose elements are m"

Cantor did not have a general membership symbol — that came later.

### 10.2 Peano's Notation (1889–1908)

Giuseppe Peano introduced much of modern logic notation:
- `ε` (epsilon) for membership — later became `∈` (from Greek "ἐστί", "is")
- `∪` for union (adopted from Grassmann, 1888)
- `∩` for intersection (adopted from Grassmann, 1888)
- `⊃` for "contains as subset" (implication dual)
- `∈` was specifically his ε for membership
- `Cls` for "class" (set)
- `ι` (iota) for "the unique x such that" (definite description)

**Peano's comprehension notation:**
```
{x ε K : P(x)}   -- the class of x in K satisfying P
```

### 10.3 Frege's Begriffsschrift (1879)

Frege's notation was radically two-dimensional:

```
    ──── B
─|──
    ──── A
```

Read as: "if A then B" (material conditional). The vertical stroke is assertion, the branching indicates conditionality. His notation:
- Had no precedence rules (branching structure was explicit)
- Used spatial position instead of parentheses
- Was a tree notation in 2D space
- Was largely ignored despite being logically superior

**Advantage:** Frege's notation made the parse tree visually obvious. Main connectives were always at branch points, subformulas were always subtrees. No ambiguity, no operator precedence.

**Disadvantage:** Impossible to type linearly, very space-inefficient.

### 10.4 Russell and Whitehead's Principia Mathematica (1910–1913)

Principia used Peano's notation but added:
- `!` for predicative functions
- `(x)` for universal quantifier (later ∀)
- `(∃x)` for existential quantifier
- Dot notation for grouping: `p . q ⊃ r` means `(p ∧ q) → r`
- Type superscripts for type levels
- `α'β` for the image of β under relation α

**Russell's comprehension:**
```
x̂(φ!x)   -- the class of x satisfying φ
```

The hat `^` over a variable indicated abstraction (later → lambda calculus).

### 10.5 Schröder's Algebra of Logic (1890)

Ernst Schröder introduced subset notation (⊆) and worked in an algebraic tradition:
- `a₁` for complement
- `0` for empty class, `1` for universal class
- `a + b` for union (Boolean sum)
- `a × b` or `ab` for intersection (Boolean product)
- `a ≤ b` for subset (inclusion as ordering)

**Key insight:** Schröder's algebraic notation — using `+`, `×`, `0`, `1`, `≤` — is what boolean algebra still uses today. Treating sets as an ordered algebra with sum, product, zero, and one is a powerful alternative framing.

### 10.6 Quine's New Foundations / Alternative Set Theories

W.V.O. Quine's NF (New Foundations) and other alternative set theories have used:
- `{x | φ}` — Quine's comprehension (unrestricted in NF)
- Stratification conditions on formulas

Von Neumann-Bernays-Gödel (NBG) set theory distinguishes:
- **Sets** (can be members of classes)
- **Proper classes** (too large to be members)

This distinction maps to the programming concern of collection types vs. type universes.

---

## 11. Constraint Logic Programming Notations

### 11.1 SMT-LIB / Z3 (S-expression Syntax)

Z3 and SMT solvers use Lisp-style prefix notation:

```lisp
; Declare a set as a predicate:
(declare-fun A (Int) Bool)   ; A is a set of integers

; Express membership:
(A 5)                        ; 5 ∈ A

; Express subset:
(assert (forall ((x Int)) (=> (A x) (B x))))   ; A ⊆ B

; Express union as disjunction:
(assert (forall ((x Int)) (= (C x) (or (A x) (B x)))))   ; C = A ∪ B

; Quantifiers:
(assert (forall ((x Int) (y Int)) ...))
(assert (exists ((x Int)) ...))
```

**Key insight:** In SMT-LIB, sets are predicates (unary functions to Bool). Set operations become logical operations (and, or, not). This is the Lean/type-theoretic view made explicit.

### 11.2 CLP(SET) / {log} (setlog)

Constraint logic programming with built-in set constraints:

```prolog
% Set membership constraint:
in(X, S)          % X ∈ S
nin(X, S)         % X ∉ S

% Set operations as constraints:
un(A, B, C)       % C = A ∪ B
inters(A, B, C)   % C = A ∩ B
diff(A, B, C)     % C = A \ B
subset(A, B)      % A ⊆ B
disj(A, B)        % A ∩ B = ∅

% Set construction using {element | rest} notation:
{1, 2 | Rest}     % set containing 1, 2, and everything in Rest

% Restricted Intensional Sets (RIS):
{X in Domain | Filter(X)}    % intensional set definition
```

**Key insight:** {log} uses Prolog-style predicate notation where set operations are constraints with output as a third argument: `un(A, B, C)` means C = A ∪ B. This is the relational/constraint style.

---

## 12. Summary: Design Patterns for Readable Set Notation

Drawing on all the above, here are the main patterns for readable set-theoretic notation in a programming language:

### Pattern A: English Keywords

```
x in S             -- membership
A subset of B      -- subset
A union B          -- union
A intersect B      -- intersection
all x in S: P(x)   -- universal
some x in S: P(x)  -- existential
no x in S: P(x)    -- none
```

**Examples:** SETL, Alloy (for some quantifiers), SQL (WHERE, UNION, EXCEPT)

### Pattern B: Comprehension with Keyword Guards

```
{ f(x) for x in S where P(x) }   -- Python-inspired
{ f(x) | x <- S, P(x) }          -- Haskell-inspired
{ f(x) : x in S | P(x) }         -- SETL/TLA+-inspired
for x in S if P(x) yield f(x)    -- Scala-inspired
```

### Pattern C: Method Chain / Fluent Interface

```
S.filter(P).map(f)
S.where(P).select(f)
S->select(x | P(x))->collect(x | f(x))   -- OCL
S.filter { it > 3 }.map { it * 2 }       -- Kotlin
```

### Pattern D: Relational / Logic Rules

```
result(f(X)) :- source(X), P(X).          -- Datalog
result = { f(x) | source(x) ∧ P(x) }     -- set comprehension as rule
```

### Pattern E: Type-as-Set / Constraint Style

```
x : S             -- x has type S (membership as typing)
x : { n : Int | n > 0 }    -- subset type / refinement type
```

### Pattern F: Algebraic / Operator Syntax

```
A + B     -- union (Alloy, Schröder, Boolean algebra)
A & B     -- intersection (Alloy, Python bitwise)
A - B     -- difference (Alloy, Python)
A * B     -- Cartesian product (some formal systems)
A | B     -- union (SQL, bitwise operators)
```

---

## Key Takeaways for Evident Language Design

1. **The `in` keyword** is universally readable for membership: `x in S`. Only math uses ∈.

2. **Five quantifiers beat two:** Alloy's `all`/`some`/`no`/`one`/`lone` is much more expressive than just ∀/∃ and reduces negation complexity.

3. **Comprehension order matters:** Programming languages agree that `for x in S where P(x) yield f(x)` reads better than the mathematical `{f(x) | x ∈ S, P(x)}` because it follows the temporal/computational order: start with source, then filter, then transform.

4. **Set operations as words or simple operators:** `union`/`intersect`/`minus` as keywords (SQL style), or `+`/`&`/`-` as infix operators (Alloy/Schröder style), both beat the Unicode ∪/∩/\.

5. **The pipe `|` is ambiguous** — it means "such that" in set comprehensions, OR in alternatives (BNF), absolute value, and boolean OR. TLA+'s colon `:` or SETL's `|` after `:` binding are cleaner.

6. **Fluent method chains** (OCL's `->select`, LINQ's `.Where()`) work well for sequential transformations but become verbose for simple filters.

7. **Interval notation `a..b`** is widely understood and unambiguous for integer ranges. The question is whether the upper bound is inclusive.

8. **Relational join via dot** (Alloy's `a.relation`) is extremely concise for navigating relations, but takes getting used to.

9. **Sets as predicates** (Lean, SMT-LIB) unifies membership with function application: `P(x)` and `x ∈ P` become the same thing. This is powerful but unusual.

10. **The Schröder framing** — treating set union as `+`, intersection as `×` or `*`, complement as `¬`, empty set as `0`, universe as `1` — connects set theory to boolean algebra and makes algebraic laws (distributivity, De Morgan) visually obvious.

---

## Sources

- [Alloy: A Lightweight Object Modelling Notation](https://dl.acm.org/doi/10.1145/505145.505149)
- [Alloy Documentation: Sets and Relations](https://alloy.readthedocs.io/en/latest/language/sets-and-relations.html)
- [TLA+ in Practice and Theory](https://pron.github.io/posts/tlaplus_part2)
- [Z Notation Reference Manual](https://www.cs.umd.edu/~mvz/handouts/z-manual.pdf)
- [Object Constraint Language — Wikipedia](https://en.wikipedia.org/wiki/Object_Constraint_Language)
- [SPARQL 1.1 Query Language](https://www.w3.org/TR/sparql11-query/)
- [Earliest Uses of Symbols of Set Theory and Logic](https://math.hawaii.edu/~tom/history/set.html)
- [SETL — Wikipedia](https://en.wikipedia.org/wiki/SETL)
- [List Comprehension — Wikipedia](https://en.wikipedia.org/wiki/List_comprehension)
- [Comparison of List Comprehensions — Wikipedia](https://en.wikipedia.org/wiki/Comparison_of_programming_languages_(list_comprehension))
- [Dependent Type — Wikipedia](https://en.wikipedia.org/wiki/Dependent_type)
- [Sets in Lean — Logic and Proof](https://leanprover-community.github.io/logic_and_proof/sets_in_lean.html)
- [Relational Algebra — Wikipedia](https://en.wikipedia.org/wiki/Relational_algebra)
- [Begriffsschrift — Wikipedia](https://en.wikipedia.org/wiki/Begriffsschrift)
- [Euler Diagram — Wikipedia](https://en.wikipedia.org/wiki/Euler_diagram)
- [Constraint (Logic) Programming with Sets — CLP(SET)](https://www.clpset.unipr.it/)
- [Answer Set Programming — Wikipedia](https://en.wikipedia.org/wiki/Answer_set_programming)
- [Homotopy Type Theory — Wikipedia](https://en.wikipedia.org/wiki/Homotopy_type_theory)
- [Curry-Howard Correspondence — Wikipedia](https://en.wikipedia.org/wiki/Curry%E2%80%93Howard_correspondence)
- [Kleene Algebra — Wikipedia](https://en.wikipedia.org/wiki/Kleene_algebra)
- [Interval (mathematics) — Wikipedia](https://en.wikipedia.org/wiki/Interval_(mathematics))
- [B-Method — Wikipedia](https://en.wikipedia.org/wiki/B-Method)
- [Formal Software Design with Alloy 6](https://haslab.github.io/formal-software-design/overview/index.html)
- [OCL Definitive Guide](https://modeling-languages.com/wp-content/uploads/2012/03/OCLChapter.pdf)
- [Programming Z3](https://theory.stanford.edu/~nikolaj/programmingz3.html)
- [Survey of Controlled Natural Languages](https://direct.mit.edu/coli/article/40/1/121/1455/A-Survey-and-Classification-of-Controlled-Natural)
- [Clojure.set API](https://clojure.github.io/clojure/clojure.set-api.html)
