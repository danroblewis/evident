# Syntactic Sugar for Set/Collection Comprehensions

*Research for Evident language design — 2026-04-26*

This document surveys how programming languages express the idea of "the set of things satisfying a condition" or "transform each element of a collection." The goal is to inform Evident's syntax for manipulating sets under constraints.

---

## 1. Mathematical Set-Builder Notation (the original)

The canonical form from mathematics:

```
{ f(x) | x ∈ S, P(x) }
```

Read: "the set of all f(x), where x is drawn from S, satisfying P(x)."

Multiple generators:

```
{ (x, y) | x ∈ A, y ∈ B, x ≠ y }
```

This is the Cartesian product of A and B, filtered. Each additional generator to the right of the `|` is either a membership assertion (`x ∈ S`) or a guard predicate (`P(x)`).

### Analysis

| Dimension | Rating |
|-----------|--------|
| Readability to outsider | High — taught in school |
| Multiple generators | Natural: comma-separated on right of `\|` |
| Guards/filters | Natural: predicate after comma |
| Grouping/aggregation | Not expressible (sets have no grouping primitive) |
| Desugaring | Defines sets axiomatically; no imperative desugaring |

The mathematical form makes the **result type** (`{ ... }`) and the **source** (`x ∈ S`) visually prominent. The guard (`P(x)`) is a peer of the generator, not syntactically subordinate to it.

---

## 2. Python Comprehensions

Python has four comprehension forms, all sharing the same grammar skeleton.

### List comprehension

```python
[x for x in S if P(x)]
[f(x) for x in S if P(x)]
```

### Set comprehension

```python
{x for x in S if P(x)}
{f(x) for x in S if P(x)}
```

### Dict comprehension

```python
{k: v for k, v in pairs}
{k: f(k) for k in S if P(k)}
```

### Generator expression

Lazy; produces elements one at a time without building a collection in memory:

```python
(x for x in S if P(x))
sum(x*x for x in range(100) if x % 2 == 0)
```

### Nested comprehensions

Multiple `for` clauses produce a cross product (left-to-right, outer-to-inner):

```python
# All pairs (x, y) where x != y
[(x, y) for x in A for y in B if x != y]

# Equivalent to:
result = []
for x in A:
    for y in B:
        if x != y:
            result.append((x, y))
```

A comprehension in the expression position produces nested collections:

```python
# Matrix transposition
[[row[i] for row in matrix] for i in range(4)]
```

### `if` guard vs. nested `for`

The `if` clause filters; a nested `for` iterates. They compose freely:

```python
[f(x, y) for x in A if P(x) for y in B if Q(y)]
```

This reads left-to-right: outer loop, outer guard, inner loop, inner guard, then the expression at the front.

### Desugaring

Python comprehensions desugar to a loop with accumulation, but they are actually implemented as a mini-function to control scoping (the loop variable does not leak):

```python
# [f(x) for x in S if P(x)]  desugars roughly to:
_result = []
for x in S:
    if P(x):
        _result.append(f(x))
```

Generator expressions desugar to a generator function:

```python
# (f(x) for x in S if P(x))  desugars to:
def _gen():
    for x in S:
        if P(x):
            yield f(x)
```

### Why Python comprehensions are praised

1. **Result type is at the front**: `[...] ` vs `{...}` immediately signals the output structure.
2. **Left-to-right reading**: `f(x) for x in S if P(x)` — expression, then source, then condition. This matches how people narrate transformations ("give me f(x) for each x in S where P holds").
3. **Guard is a plain `if`**: no special keyword, no method call, just a condition.
4. **Composable**: the `for`/`if` clauses stack without extra syntax.
5. **Symmetry between forms**: list, set, dict, and generator differ only in the outer delimiters and the expression position (`k: v` for dict).

### Analysis

| Dimension | Assessment |
|-----------|------------|
| Readability to outsider | Very high once `for x in S` is understood |
| Multiple generators | `for x in A for y in B` — readable, left-to-right |
| Guards/filters | `if P(x)` — natural |
| Grouping/aggregation | Not built-in; use `itertools.groupby` outside the comprehension |
| Desugaring | Simple loop with append |

---

## 3. Haskell List Comprehensions

```haskell
[x | x <- xs, P x]
[f x | x <- xs, P x]
```

The `|` mirrors mathematical set-builder notation. Left of `|` is the expression; right is a comma-separated list of **generators** (`x <- xs`) and **guards** (`P x`).

### Multiple generators

```haskell
-- Cross product, filtered
[(x, y) | x <- xs, y <- ys, x /= y]

-- Pythagorean triples up to n
[(a, b, c) | c <- [1..n], b <- [1..c], a <- [1..b], a^2 + b^2 == c^2]
```

Generators to the right vary faster (rightmost is innermost loop).

### Guards

Guards are plain boolean expressions anywhere in the generator list:

```haskell
[x | x <- [1..100], even x, x `mod` 3 == 0]
```

### Let bindings inside comprehensions

```haskell
[(x, y, dist) | x <- points, y <- points,
                let dist = distance x y,
                dist < threshold]
```

`let` introduces a local binding scoped to the rest of the comprehension.

### Pattern matching in the generator position

Failed pattern matches are silently skipped (like a built-in guard):

```haskell
-- Extract only Just values
[x | Just x <- maybes]

-- Extract left components from Either
[x | Left x <- eithers]
```

### Parallel comprehensions (GHC extension)

```haskell
{-# LANGUAGE ParallelListComp #-}
[(x, y) | x <- xs | y <- ys]  -- zip, not cross-product
```

Parallel `|` separators zip generators instead of crossing them.

### How they desugar to `do` notation

```haskell
-- [f x | x <- xs, P x]  desugars to:
do
  x <- xs
  guard (P x)
  return (f x)

-- Which in terms of >>= is:
xs >>= \x -> if P x then [f x] else []
```

This means list comprehensions generalize to any monad by replacing `[...]` with the appropriate monad's comprehension (enabled by `MonadComprehensions` extension).

### Analysis

| Dimension | Assessment |
|-----------|------------|
| Readability to outsider | Moderate — `<-` is unfamiliar; `\|` is familiar from math |
| Multiple generators | Natural comma separation, rightmost is innermost |
| Guards/filters | Plain booleans mixed with generators |
| Grouping/aggregation | Not built-in; use `Data.List.groupBy` outside |
| Desugaring | `>>=` / `do` notation / `guard` |

---

## 4. C# LINQ Query Syntax

LINQ (Language Integrated Query) introduced SQL-like comprehension syntax into C# in 2007.

### Basic form

```csharp
from x in S
where P(x)
select f(x)
```

### Multiple generators (cross product / join)

```csharp
from x in A
from y in B
where x != y
select (x, y)
```

Each additional `from` is an additional generator (like `for` in Python or `<-` in Haskell).

### Let bindings

```csharp
from x in S
let y = transform(x)
where P(y)
select y
```

### Join

```csharp
from order in orders
join customer in customers on order.CustomerId equals customer.Id
select new { order, customer }
```

### Group by

```csharp
from product in products
group product by product.Category into g
select new { Category = g.Key, Products = g.ToList() }
```

### Order by

```csharp
from x in S
where P(x)
orderby x.Name ascending
select x
```

### Method chain equivalent

Every query-syntax form has a method-chain equivalent:

```csharp
// Query syntax:
from x in S where P(x) select f(x)

// Method chain:
S.Where(P).Select(f)

// More complex:
S.Where(x => x.Active)
 .GroupBy(x => x.Category)
 .Select(g => new { g.Key, Count = g.Count() })
```

The compiler translates query syntax to method calls. The two forms are equivalent.

### Why LINQ was influential

1. **SQL familiarity**: developers already knew `SELECT ... FROM ... WHERE`.
2. **Type safety**: unlike SQL strings, LINQ is checked at compile time.
3. **Works on any `IEnumerable<T>`**: in-memory collections, databases (LINQ to SQL, Entity Framework), XML (LINQ to XML), and any custom provider.
4. **Composable**: query expressions compose; results are sequences that can feed into further queries.
5. **Dual syntax**: programmers choose query syntax for complex queries (especially joins/groups) and method chains for simple transformations.

### The "inside-out" ordering problem

SQL's `SELECT ... FROM ... WHERE` and LINQ's `from ... where ... select` differ on where the expression appears. LINQ puts `from` first so that IntelliSense can suggest completions for `x` after the type is known. SQL puts `SELECT` first, matching math notation, but editors must defer completion until the `FROM` clause is parsed.

### Analysis

| Dimension | Assessment |
|-----------|------------|
| Readability to outsider | Very high for SQL-familiar developers |
| Multiple generators | `from x in A from y in B` — clear |
| Guards/filters | `where P(x)` — SQL-familiar keyword |
| Grouping/aggregation | Full `group by`, `having`-equivalent via `where` after `into` |
| Desugaring | Method calls: `.Where()`, `.Select()`, `.GroupBy()`, `.Join()` |

---

## 5. Scala For-Comprehensions

Scala's `for` is a general-purpose comprehension that desugars to method calls.

### Basic form

```scala
for (x <- xs if P(x)) yield f(x)

// Block syntax (preferred for multiple generators):
for {
  x <- xs
  if P(x)
} yield f(x)
```

### Multiple generators

```scala
for {
  x <- A
  y <- B
  if x != y
} yield (x, y)
```

### Let bindings

```scala
for {
  x <- xs
  y = transform(x)   // not "<-", just "="
  if P(y)
} yield y
```

### Pattern matching in generators

```scala
for {
  (k, v) <- map.toList  // destructure tuples
  if v > 0
} yield k -> v * 2
```

Failed patterns are filtered out when the type supports `withFilter`.

### Desugaring to flatMap/map/filter

```scala
// for { x <- xs; if P(x) } yield f(x)
// desugars to:
xs.withFilter(P).map(f)

// for { x <- xs; y <- ys } yield (x, y)
// desugars to:
xs.flatMap(x => ys.map(y => (x, y)))

// for { x <- xs; y = g(x); if P(y) } yield f(y)
// desugars to:
xs.map(x => (x, g(x))).withFilter { case (_, y) => P(y) }.map { case (_, y) => f(y) }
```

Because desugaring targets method names (`map`, `flatMap`, `withFilter`), for-comprehensions work on **any type** that defines those methods: `List`, `Option`, `Future`, `Either`, custom monads.

### Why Scala has both comprehension syntax and method chaining

- For-comprehensions are preferred when there are multiple generators, let bindings, or nested flatMaps (they avoid deep nesting).
- Method chains are preferred for simple, single-step transformations where the chain reads naturally.
- They are exactly equivalent; choice is stylistic.

### Analysis

| Dimension | Assessment |
|-----------|------------|
| Readability to outsider | Moderate — `<-` and `yield` are unfamiliar |
| Multiple generators | Clean block syntax with one binding per line |
| Guards/filters | `if P(x)` inside the block |
| Grouping/aggregation | Not built-in; use `.groupBy()` outside |
| Desugaring | `flatMap` / `map` / `withFilter` |

---

## 6. F# Computation Expressions

F# generalizes comprehensions to arbitrary monads via **computation expressions**. The sequence comprehension is the most common example.

### Sequence expressions

```fsharp
seq {
    for x in S do
        if P x then
            yield f x
}
```

Or more concisely:

```fsharp
seq { for x in S do if P x then yield f x }
```

### Multiple generators

```fsharp
seq {
    for x in A do
        for y in B do
            if x <> y then
                yield (x, y)
}
```

### yieldAll (flatten)

```fsharp
seq {
    for xs in nested do
        yield! xs   // flatten one level
}
```

### List and array expressions

```fsharp
[ for x in S do if P x then yield f x ]
[| for x in S do if P x then yield f x |]  // array
```

### Generalizing to arbitrary monads

F# computation expressions allow any type to define a builder object with methods like `Bind`, `Return`, `Yield`, `For`, etc. The `{ ... }` block then desugars to calls on that builder:

```fsharp
// async computation expression
async {
    let! x = fetchAsync url   // Bind
    let y = transform x       // Let
    return y                  // Return
}

// result/option computation expression
maybe {
    let! x = tryGetValue key
    let! y = tryParse x
    return x + y
}
```

This means F#'s comprehension syntax is really a general monad syntax, not special-cased to lists/sequences.

### Analysis

| Dimension | Assessment |
|-----------|------------|
| Readability to outsider | Low for general form; `for x in S do yield` is approachable |
| Multiple generators | Nested `for ... do` blocks — readable but verbose |
| Guards/filters | Plain `if` — natural |
| Grouping/aggregation | Not built-in for seq; custom builders can express it |
| Desugaring | Builder methods: `Bind`, `Yield`, `For`, `YieldFrom` |

---

## 7. Erlang and Elixir List Comprehensions

### Erlang

```erlang
[f(X) || X <- S, P(X)]
```

Multiple generators and guards are comma-separated after `||`:

```erlang
[{X, Y} || X <- A, Y <- B, X =/= Y]
```

Bitstring generators (unique to Erlang) iterate over binary data:

```erlang
[Byte || <<Byte>> <= Binary, Byte > 0]
```

The `<=` generator extracts bytes from a binary. The result can also be a bitstring:

```erlang
<< <<Byte>> || <<Byte>> <= Binary, Byte > 0 >>
```

### Elixir

```elixir
for x <- S, P(x), do: f(x)
```

Multiple generators:

```elixir
for x <- A, y <- B, x != y, do: {x, y}
```

Bitstring generators:

```elixir
for <<byte <- binary>>, byte > 0, do: byte
```

The `into:` option controls the output collection type:

```elixir
for x <- S, P(x), into: %{}, do: {x, f(x)}   # produces a map
for x <- S, P(x), into: "",  do: to_string(x)  # produces a string
```

This `into:` pattern is notable: it separates **where elements come from** and **what condition they satisfy** from **what structure they accumulate into**. Most languages bake the output type into the comprehension form's delimiters; Elixir makes it an explicit parameter.

### Analysis

| Dimension | Assessment |
|-----------|------------|
| Readability to outsider | Erlang: `\|\|` is unusual; Elixir: very readable |
| Multiple generators | Comma-separated, flat list — clean |
| Guards/filters | Plain predicates in the comma list — elegant |
| Grouping/aggregation | Not built-in; `Enum.group_by/2` outside |
| Desugaring | Nested loops with accumulator |

---

## 8. SQL SELECT as Comprehension

SQL's `SELECT` statement is a comprehension over relations (multisets of rows).

### Basic mapping

```sql
SELECT f(x) FROM S WHERE P(x)
```

This is exactly `{ f(x) | x ∈ S, P(x) }`.

### Multiple generators (cross product and join)

```sql
-- Cross product:
SELECT x.name, y.name
FROM employees x, employees y
WHERE x.dept = y.dept AND x.id < y.id

-- Explicit join (same semantics, cleaner syntax):
SELECT o.id, c.name
FROM orders o
JOIN customers c ON o.customer_id = c.id
```

### Grouping and aggregation

`GROUP BY` extends the comprehension model with a grouping dimension:

```sql
SELECT category, COUNT(*), AVG(price)
FROM products
WHERE active = true
GROUP BY category
HAVING COUNT(*) > 5
ORDER BY AVG(price) DESC
```

This is not expressible in pure set-builder notation. It maps a multiset to a set of groups, each with aggregate values. `HAVING` is a guard applied *after* grouping (equivalent to filtering the output of `GROUP BY`).

### Subqueries as nested comprehensions

```sql
SELECT name
FROM employees
WHERE dept_id IN (
    SELECT id FROM departments WHERE budget > 1000000
)
```

### The "inside-out" ordering problem

Mathematical notation writes `{ SELECT | FROM, WHERE }`. SQL writes `SELECT ... FROM ... WHERE ...`. The expression comes first, but the bindings that introduce variable names come after. This means:

1. You cannot write the `SELECT` expression until you know what `FROM` names are in scope.
2. IDEs must do two-pass parsing to offer completion in `SELECT`.
3. Conceptual reading order is `FROM → WHERE → SELECT`, but textual order is `SELECT → FROM → WHERE`.

LINQ's `from x in S where P(x) select f(x)` fixes this by putting the generator first. SQL keeps the expression first for familiarity and because it emphasizes "what you want" before "where it comes from."

### Analysis

| Dimension | Assessment |
|-----------|------------|
| Readability to outsider | Very high — SQL is the most-known query language |
| Multiple generators | `FROM A, B` or `JOIN` — well-understood |
| Guards/filters | `WHERE` — extremely familiar |
| Grouping/aggregation | Full `GROUP BY` / `HAVING` / aggregate functions |
| Desugaring | Relational algebra: σ (select), π (project), ⋈ (join), γ (group) |

---

## 9. Kotlin Sequence Builders

Kotlin's `sequence { }` builder creates a lazy sequence using coroutines. It is closer to a generator than a comprehension, but serves the same purpose.

### Basic form

```kotlin
val evens = sequence {
    var n = 0
    while (true) {
        yield(n)
        n += 2
    }
}

val filtered = sequence {
    for (x in source) {
        if (P(x)) yield(f(x))
    }
}
```

### yieldAll

```kotlin
val combined = sequence {
    yieldAll(firstCollection)
    yieldAll(secondCollection)
    yield(singleElement)
}
```

### Method chain alternative

Kotlin also supports standard functional chains via extension functions on `Sequence<T>` and `Iterable<T>`:

```kotlin
source.filter { P(it) }.map { f(it) }.toList()

// Cross product:
A.flatMap { x -> B.map { y -> x to y } }.filter { (x, y) -> x != y }
```

### Relationship to comprehensions

Kotlin does not have built-in comprehension syntax (no `for ... yield`). The `sequence { }` builder is the closest equivalent for lazy evaluation. For eager collections, method chains are idiomatic.

### Analysis

| Dimension | Assessment |
|-----------|------------|
| Readability to outsider | `sequence { for (x in S) if (P(x)) yield(f(x)) }` is readable |
| Multiple generators | Nested `for` loops inside the builder |
| Guards/filters | `if` inside the block |
| Grouping/aggregation | Not built-in; `.groupBy()` outside |
| Desugaring | Coroutine suspension at each `yield` |

---

## 10. Rust Iterator Adapters

Rust does not have comprehension syntax. Instead, it uses **iterator adapter chains** — method calls that compose lazily.

### Basic transformation and filtering

```rust
let result: Vec<_> = S.iter()
    .filter(|x| P(x))
    .map(|x| f(x))
    .collect();
```

### Multiple generators (cross product)

```rust
use itertools::iproduct;

let pairs: Vec<_> = iproduct!(A.iter(), B.iter())
    .filter(|(x, y)| x != y)
    .collect();

// Or with flat_map:
let pairs: Vec<_> = A.iter()
    .flat_map(|x| B.iter().map(move |y| (x, y)))
    .filter(|(x, y)| x != y)
    .collect();
```

### The `collect()` into-type idiom

`collect()` is type-directed: the return type annotation determines what collection is built:

```rust
let set: HashSet<_> = S.iter().filter(|x| P(x)).cloned().collect();
let map: HashMap<_, _> = S.iter().map(|x| (x.key(), x.val())).collect();
let string: String = chars.filter(|c| c.is_alphabetic()).collect();
```

This is powerful: the same chain produces different collection types based on the type annotation. It separates **what to compute** from **what to store it in** — analogous to Elixir's `into:`.

### Grouping

```rust
use itertools::Itertools;

let groups = S.iter().group_by(|x| x.category());
for (key, group) in &groups {
    let sum: i32 = group.map(|x| x.value()).sum();
    println!("{}: {}", key, sum);
}
```

### Why no comprehension syntax?

The Rust community has discussed adding comprehension syntax multiple times. The main reasons it has not been added:

1. Iterator chains are already highly composable and readable to Rust developers.
2. Comprehensions would be syntactic sugar with no additional expressiveness.
3. The `flat_map` pattern handles nested iteration; explicit syntax might obscure the complexity of borrow-checking across closures.
4. A proposed `vec![x | x <- xs, P(x)]` macro was explored but not merged.

### Analysis

| Dimension | Assessment |
|-----------|------------|
| Readability to outsider | Low — chains of `.filter().map().collect()` are unfamiliar |
| Multiple generators | `flat_map` — not intuitive for newcomers |
| Guards/filters | `.filter(\|x\| P(x))` — verbose but explicit |
| Grouping/aggregation | Via `itertools::group_by`, `.fold()`, `.sum()` etc. |
| Desugaring | IS the desugaring — it is already the primitive form |

---

## Comparison Table

| Language | Syntax Form | Output Type Declared | Guard Syntax | Multiple Generators | Grouping | Lazy |
|----------|-------------|---------------------|--------------|--------------------|---------|----|
| Math | `{ f(x) \| x ∈ S, P(x) }` | Implicit (set) | Predicate after `,` | `x ∈ A, y ∈ B` | No | N/A |
| Python (list) | `[f(x) for x in S if P(x)]` | Explicit (`[]`) | `if P(x)` | `for x in A for y in B` | No | No |
| Python (set) | `{f(x) for x in S if P(x)}` | Explicit (`{}`) | `if P(x)` | `for x in A for y in B` | No | No |
| Python (gen) | `(f(x) for x in S if P(x))` | Explicit (`()`) | `if P(x)` | `for x in A for y in B` | No | Yes |
| Haskell | `[f x \| x <- xs, P x]` | Implicit (list) | Predicate after `,` | `x <- xs, y <- ys` | No | Yes (lazy list) |
| C# LINQ | `from x in S where P(x) select f(x)` | Inferred | `where P(x)` | `from x in A from y in B` | `group by` / `into` | Deferred |
| Scala | `for { x <- xs; if P(x) } yield f(x)` | Inferred | `if P(x)` | `x <- A; y <- B` | No | Depends on source |
| F# | `seq { for x in S do if P x then yield f x }` | Named builder | `if P x` | Nested `for ... do` | No | Yes (seq) |
| Erlang | `[f(X) \|\| X <- S, P(X)]` | Implicit (list) | Predicate after `,` | `X <- A, Y <- B` | No | No |
| Elixir | `for x <- S, P(x), do: f(x)` | `into:` option | Predicate in list | `x <- A, y <- B` | No | No |
| SQL | `SELECT f(x) FROM S WHERE P(x)` | Schema | `WHERE P(x)` | `FROM A, B` / `JOIN` | `GROUP BY` / `HAVING` | No |
| Kotlin seq | `sequence { for (x in S) if (P(x)) yield(f(x)) }` | Named builder | `if` inside block | Nested `for` | No | Yes |
| Rust | `S.iter().filter(\|x\| P(x)).map(\|x\| f(x)).collect()` | Type annotation | `.filter(...)` | `.flat_map(...)` | `itertools` | Yes (lazy chain) |

---

## Key Design Observations for Evident

### 1. Expression-first vs. generator-first

Two camps exist:

- **Expression first** (math, SQL): `{ f(x) | x ∈ S, P(x) }`, `SELECT f(x) FROM S WHERE P(x)`. The result expression is written before the source. Reads as "give me f(x), for x in S, where P holds." Emphasizes the output.
- **Generator first** (LINQ, Haskell, Python): `from x in S where P(x) select f(x)`, `[f(x) for x in S if P(x)]`. The source is named first, allowing tools to know what variables are in scope before the expression is written.

For a language where the programmer is specifying **what they want** (declarative / constraint-based), expression-first may be more natural. For a language where variables need to be in scope for editor support, generator-first is more practical.

### 2. Guards as peers vs. guards as subordinate

- **Peers** (Haskell, Erlang, math): generators and guards are comma-separated at the same syntactic level. Clean and minimal.
- **Subordinate** (Python `if`, Scala `if`, C# `where`, SQL `WHERE`): guards have their own keyword, distinct from generators. More readable to non-specialists.

For Evident, treating guards as peers (same comma list as generators) aligns with how constraints are expressed in constraint programming — a constraint is logically the same kind of thing as a domain restriction.

### 3. Output type specification

| Approach | Example |
|----------|---------|
| Delimiter encodes type | `[...]` list, `{...}` set, `(...)` generator (Python) |
| Named builder | `seq { }`, `async { }` (F#, Kotlin) |
| Type annotation at use site | `: HashSet<_>` (Rust) |
| `into:` option | `for ..., into: %{}` (Elixir) |
| Implicit (always same type) | Haskell list, Erlang list |

For a language whose primary data structure is **sets**, using `{ }` delimiters for set comprehensions (as in Python and math) is the natural choice.

### 4. Multiple generators = cross product

Every language expresses multiple generators as some form of comma- or keyword-separated list. The cross-product semantics is universal. The key design choice is:

- Are generators visually distinguished from guards? (C# `from` vs. `where`; Python `for` vs. `if`)
- Or are they peers in a uniform list? (Haskell, Erlang, math)

### 5. Grouping is orthogonal

No comprehension syntax natively supports grouping well, except SQL. Grouping requires a fundamentally different operation: collapsing multiple elements into a group and applying aggregate functions. This is beyond simple mapping/filtering. Evident may need to express grouping separately, or treat it as a higher-order operation on sets of sets.

### 6. Laziness

For Evident (constraint propagation over sets), lazy/incremental evaluation may be important. Haskell's lazy lists and Python generators offer a model for this — the comprehension syntax remains the same, but the evaluation strategy differs.

### 7. The most readable core form

Combining the best of math notation and Python/Haskell:

```
{ f(x) | x ∈ S, P(x) }
```

This form:
- Uses `{ }` to signal a set result
- Uses `|` to separate result expression from generators/guards
- Uses `∈` (or `in`) for membership
- Uses plain predicates as guards (no extra keyword)
- Composes via comma separation

A more ASCII-friendly version:

```
{ f(x) | x in S, P(x) }
```

Or with multiple generators:

```
{ (x, y) | x in A, y in B, x != y }
```

This is the design space where Evident's comprehension syntax should live.
