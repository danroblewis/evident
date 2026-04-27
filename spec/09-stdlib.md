# Evident Specification — Standard Library

The standard library is a set of claims and types available to every Evident
program without import. It divides into two layers:

- **Primitive claims** — built into the solver. They cannot be expressed in
  Evident itself because they correspond to the solver's own reasoning
  capabilities (arithmetic, cardinality, fixed-point computation). They are
  listed here with their interface; their implementation is the solver.

- **Library claims** — definable in Evident. They are provided as a convenience.
  Their bodies are valid Evident, shown below. They could be written by any user;
  they are in the standard library only because they appear in nearly every
  program.

Each entry is marked **(primitive)** or **(library)** to distinguish the two.

---

## Numeric

### `sum_of` (primitive)

The total of a finite set of natural numbers.

```evident
claim sum_of
    items ∈ Set Nat
    total ∈ Nat
    total = Σ items
```

`Σ` is a built-in primitive operator. The solver computes sums directly. This
claim is provided as a named interface over that operation.

Generalises to `Int` and `Real` with the same interface.

### `max_of` (library)

The largest element of a non-empty set.

```evident
claim max_of[T ∈ Ordered]
    items ∈ Set T
    m     ∈ T
    m ∈ items
    ∀ x ∈ items : x ≤ m
```

Requires `T ∈ Ordered`. The claim is semidet when `items` is non-empty; it has
no solution when `items` is empty (no largest element of the empty set exists).

### `min_of` (library)

The smallest element of a non-empty set.

```evident
claim min_of[T ∈ Ordered]
    items ∈ Set T
    m     ∈ T
    m ∈ items
    ∀ x ∈ items : m ≤ x
```

Symmetric to `max_of`. Semidet; no solution for the empty set.

### `within_range` (library)

A value falls between a lower and upper bound (inclusive).

```evident
claim within_range[T ∈ Ordered]
    value ∈ T
    lower ∈ T
    upper ∈ T
    lower ≤ value
    value ≤ upper
```

Bidirectional: can check whether `value` is in range, or generate values between
`lower` and `upper` when `value` is unbound.

### `absolute_difference` (primitive)

The absolute difference between two values.

```evident
claim absolute_difference
    a    ∈ Int
    b    ∈ Int
    diff ∈ Nat
    diff = |a - b|
```

`|·|` for absolute value is a primitive numeric operator, distinct from `|S|`
cardinality (distinguished by context: `|n|` for numeric values, `|S|` for sets).

---

## Set structure

### `all_different` (library)

No element appears more than once in a list.

```evident
claim all_different[T ∈ Eq]
    items ∈ List T
    |{ v | (_, v) ∈ items }| = |items|
```

The set-comprehension collapses duplicates; if it has the same cardinality as the
list, all elements were distinct.

### `at_most` (library)

A set has no more than `n` elements.

```evident
claim at_most
    n     ∈ Nat
    items ∈ Set T
    |items| ≤ n
```

### `at_least` (library)

A set has at least `n` elements.

```evident
claim at_least
    n     ∈ Nat
    items ∈ Set T
    |items| ≥ n
```

### `exactly` (library)

A set has exactly `n` elements.

```evident
claim exactly
    n     ∈ Nat
    items ∈ Set T
    |items| = n
```

`exactly` is definitionally equivalent to `at_most n items, at_least n items`.
It is provided as a single name for readability.

### `partition` (library)

A collection of groups that covers every element of a set exactly once.

```evident
claim partition[T]
    items  ∈ Set T
    groups ∈ Set (Set T)
    ∀ x ∈ items : exactly 1 { g ∈ groups | x ∈ g }
    ∀ g ∈ groups : g ⊆ items
```

Every element of `items` belongs to exactly one group. Every group is a subset
of `items`. Groups may be empty only if the entire collection is empty; an empty
group would violate the cover property for no element, so empty groups are
implicitly excluded when `items` is non-empty.

### `disjoint` (library)

Two sets share no elements.

```evident
claim disjoint[T]
    a ∈ Set T
    b ∈ Set T
    a ∩ b = ∅
```

### `covers` (library)

Every element of one set appears in another.

```evident
claim covers[T]
    source ∈ Set T
    target ∈ Set T
    source ⊆ target
```

This is a thin wrapper over `⊆` provided for readability in domain claims
(`tasks_covered_by schedule` is more readable than an inline `⊆`).

---

## Sequence ordering

These claims operate on values of type `Indexable T` (defined below). They
express ordering properties of sequences without requiring a specific list
constructor.

### `in_order` (library)

Every consecutive pair in the sequence is non-decreasing.

```evident
claim in_order[T ∈ Ordered]
    arr ∈ Indexable T
    ∀ (a, b) ∈ arr.consecutive_pairs : a ≤ b
```

### `strictly_in_order` (library)

Every consecutive pair is strictly increasing.

```evident
claim strictly_in_order[T ∈ Ordered]
    arr ∈ Indexable T
    ∀ (a, b) ∈ arr.consecutive_pairs : a < b
```

`strictly_in_order` implies `all_different` for sequences.

### `no_equal_adjacent` (library)

No two adjacent elements are equal (but non-adjacent repeats are allowed).

```evident
claim no_equal_adjacent[T ∈ Eq]
    arr ∈ Indexable T
    ∀ (a, b) ∈ arr.consecutive_pairs : a ≠ b
```

Weaker than `all_different`: `[1, 2, 1]` satisfies `no_equal_adjacent` but not
`all_different`.

### `sorted` (library)

A list whose elements are non-decreasing. Equivalent to `in_order` applied to a
`List T` viewed as `Indexable T`.

```evident
claim sorted[T ∈ Ordered]
    list ∈ List T
    ∀ (a, b) ∈ list.consecutive_pairs : a ≤ b
```

`sorted` is defined as a claim (not a type) because sortedness is a property of
a value, not a structural requirement. Compare: `List T` is the type of all lists;
`sorted list` further constrains one.

---

## Relation properties

These claims characterise mathematical properties of binary relations, expressed
as subsets of `T × T`.

### `functional` (library)

Every left-hand element has at most one right-hand element (the relation is a
partial function).

```evident
claim functional[T, U]
    rel ⊆ T × U
    ∀ (x, y1) ∈ rel, (x, y2) ∈ rel : y1 = y2
```

### `total_function` (library)

The relation is a function defined on all elements of a domain.

```evident
claim total_function[T, U]
    domain ∈ Set T
    rel    ⊆ T × U
    functional rel
    ∀ x ∈ domain : ∃ (x, _) ∈ rel
```

### `injective` (library)

Every right-hand element has at most one left-hand element (no two inputs map to
the same output).

```evident
claim injective[T, U]
    rel ⊆ T × U
    ∀ (x1, y) ∈ rel, (x2, y) ∈ rel : x1 = x2
```

### `surjective` (library)

Every element of the codomain is hit by at least one element of the domain.

```evident
claim surjective[T, U]
    domain   ∈ Set T
    codomain ∈ Set U
    rel      ⊆ T × U
    ∀ y ∈ codomain : ∃ (_, y) ∈ rel
```

### `bijective` (library)

The relation is both injective and surjective — a one-to-one correspondence.

```evident
claim bijective[T, U]
    domain   ∈ Set T
    codomain ∈ Set U
    rel      ⊆ T × U
    injective rel
    surjective
        domain   ↦ domain
        codomain ↦ codomain
        rel      ↦ rel
```

### `symmetric` (library)

If `(a, b)` is in the relation, so is `(b, a)`.

```evident
claim symmetric[T]
    rel ⊆ T × T
    ∀ (a, b) ∈ rel : (b, a) ∈ rel
```

### `transitive` (library)

If `(a, b)` and `(b, c)` are in the relation, so is `(a, c)`.

```evident
claim transitive[T]
    rel ⊆ T × T
    ∀ (a, b) ∈ rel, (b, c) ∈ rel : (a, c) ∈ rel
```

---

## Graph and reachability

### `reachable` (primitive / fixed-point)

Node `b` is reachable from node `a` via a directed edge set.

```evident
claim reachable[T]
    edges ⊆ T × T
    a     ∈ T
    b     ∈ T

-- base: every node reaches itself
(a, b) ∈ edges ⇒ reachable edges a b

-- step: transitivity via intermediate node
reachable edges a c, (c, b) ∈ edges ⇒ reachable edges a b
```

`reachable` is defined by forward implication rules (closure under derivation)
rather than a single universally-quantified body. The solver computes the
transitive closure to a fixed point. This is the standard use case for the `⇒`
rule form.

Whether `reachable` is primitive or definable in Evident via `⇒` rules is a
question of what the solver supports natively. The `⇒` form with fixed-point
semantics is provided; if the solver implements it correctly, `reachable` is a
library claim. If fixed-point rule chains require special solver support, it is
primitive.

### `connected` (library)

Every node is reachable from every other node (undirected connectivity).

```evident
claim connected[T]
    nodes ∈ Set T
    edges ⊆ T × T
    symmetric edges
    ∀ a ∈ nodes, b ∈ nodes : reachable edges a b
```

### `acyclic` (library)

No node is reachable from itself (no cycles in the directed graph).

```evident
claim acyclic[T]
    nodes ∈ Set T
    edges ⊆ T × T
    ∀ n ∈ nodes : ¬ reachable edges n n
```

---

## The `Indexable` interface

`Indexable T` is a structural type — a set of records with specific fields and
constraints — that captures the notion of a finite indexed sequence. It is the
basis for `in_order`, `strictly_in_order`, `no_equal_adjacent`, and similar
sequence claims.

```evident
type Indexable T = {
    n                 ∈ Nat
    entries           ⊆ Nat × T
    consecutive_pairs ⊆ T × T

    -- entries covers exactly indices 0 through n-1
    ∀ i ∈ {0..n-1} : exactly 1 { (j, v) ∈ entries | j = i }
    ∀ (i, _) ∈ entries : i ∈ {0..n-1}

    -- consecutive_pairs is the set of value-pairs at adjacent indices
    consecutive_pairs = { (v1, v2) | (i, v1) ∈ entries, (i+1, v2) ∈ entries }
}
```

Fields:

- `n`: the length of the sequence
- `entries`: the set of `(index, value)` pairs; exactly one entry per index in
  `{0..n-1}`
- `consecutive_pairs`: the derived set of `(value_at_i, value_at_{i+1})` pairs;
  tight-bound by the defining equation

`List T` as defined in the built-in type system satisfies `Indexable T`. Any
user-defined type that provides `n`, `entries`, and `consecutive_pairs` with
these constraints also satisfies `Indexable T` — structural, not nominal.

The tight binding `consecutive_pairs = { ... }` means the solver eliminates
`consecutive_pairs` by substitution before search. Claims that quantify over
`consecutive_pairs` expand directly to the underlying `entries` membership
conditions.

---

## Arithmetic primitives

These are built into the solver. They are not definable in Evident.

| Primitive | Meaning |
|---|---|
| `a + b = c` | addition |
| `a - b = c` | subtraction (Int) |
| `a * b = c` | multiplication |
| `a / b = c` | division (Real); integer quotient for Nat/Int |
| `a mod b = c` | remainder |
| `a ≤ b` | non-strict ordering |
| `a < b` | strict ordering |
| `a = b` | equality (for all types in `Eq`) |
| `Σ S` | sum of a finite set S ⊆ Nat (or Int, Real) |
| `|n|` | absolute value |
| `|S|` | cardinality of a finite set |
| `{a..b}` | integer range (inclusive) |

Arithmetic constraints are bidirectional: `a + b = c` can determine any one of
the three variables given the other two. The solver handles this without special
casing; it is a consequence of treating arithmetic as constraint propagation.

---

## Open questions

- **`reachable` and fixed-point rules**: the `⇒` rule form with forward-chaining
  semantics is under specification. Whether the solver supports arbitrary fixed-point
  closures or only restricted forms (e.g., stratified Datalog) is not yet decided.
  If restricted, `reachable` over arbitrary graphs is primitive; if unrestricted,
  it is library.

- **`Σ` generalisation**: whether `Σ` extends naturally to `Int` and `Real` without
  convergence issues is assumed but not proven for the constraint solver.

- **Cardinality for infinite sets**: `|S|` is defined only for finite sets.
  Whether the solver can statically guarantee finiteness (and reject programs where
  it cannot) or whether it fails at runtime is an open design question.

- **`{a..b}` for non-integer ranges**: whether range notation extends to `Real`
  (presumably not, since it would be uncountably infinite) and whether that should
  be a type error or a runtime error.

- **`partition` and empty groups**: the current definition allows empty groups in
  the `groups` set when `items` is empty. Whether `partition` should require
  `∀ g ∈ groups : |g| ≥ 1` (non-empty groups always) is not settled.

- **Standard library source**: whether the library claims are shipped as a
  pre-compiled Evident module, inlined by the compiler, or handled via a
  special `stdlib` namespace is an implementation question with no current answer.
