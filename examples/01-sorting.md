# Example 1: Sorting — Constraint Accumulation

This example shows the core Evident workflow: each step adds a membership condition to the
set named `sort`, intersecting it with a smaller set of pairs. We stop when the set contains
exactly the elements we want — in this case, exactly one pair for each input list.

We are not writing a sorting algorithm. We are writing a specification of what it means for
a list to be sorted, and the solver finds a sorted version.

---

## Step 0: Naming the set — no membership conditions yet

```evident
claim sort[T ∈ Ordered] : List T → List T → Prop
```

This declares that `sort` names a set of pairs `(xs, ys)` where both are `List T`. With no
`evident` blocks, membership is completely unconstrained — the set is the entire
`List T × List T`. Any pair is a member.

```evident
? sort [3, 1, 2] ?result
```

```
-- Solver may return:
result = []                -- valid (it's a List Nat)
result = [0, 0, 0]        -- valid (also a List Nat)
result = [3, 1, 2]        -- valid (also a List Nat)
result = [999]             -- valid (also a List Nat)

-- The claim 'sort xs ys' is trivially evident for any xs and ys
-- because we said nothing about the relationship between them.
-- This is not useful.
```

---

## Step 1: First intersection — pairs of equal length

```evident
claim sort[T ∈ Ordered] : List T → List T → Prop

evident sort xs ys
    length ys = length xs
```

The body condition `length ys = length xs` restricts `sort` to the subset of
`List T × List T` where the two lists have equal length. `sort` is now
`List T × List T ∩ { (xs, ys) | length xs = length ys }`.

```evident
? sort [3, 1, 2] ?result
```

```
-- Solver may return:
result = [0, 0, 0]        -- valid (length 3, correct type)
result = [3, 1, 2]        -- valid (length 3, correct type)
result = [9, 9, 9]        -- valid (length 3, correct type)

-- Better: the result is at least the right size.
-- But we haven't said anything about which elements appear, or their order.
```

---

## Step 2: Second intersection — pairs where the second list is sorted

```evident
claim sort[T ∈ Ordered] : List T → List T → Prop

evident sort xs ys
    length ys = length xs
    sorted ys
```

We need `sorted` to exist. Let's define it:

```evident
claim sorted[T ∈ Ordered] : List T → Prop

evident sorted []
evident sorted [_]
evident sorted [a, b | rest] when a ≤ b
    sorted [b | rest]
```

Adding `sorted ys` further restricts the set. `sort` is now the intersection of the
equal-length pairs with the pairs where `ys` is non-decreasing.

```evident
? sort [3, 1, 2] ?result
```

```
-- Solver may return:
result = [0, 0, 0]        -- valid (length 3, sorted)
result = [1, 1, 1]        -- valid (length 3, sorted)
result = [1, 2, 3]        -- valid (length 3, sorted)  ← coincidentally correct!
result = [1, 2, 4]        -- valid (length 3, sorted)  ← wrong elements

-- We're getting closer. The result is sorted and the right size.
-- But it can contain entirely different elements.
```

---

## Step 3: Third intersection — pairs sharing the same elements

```evident
claim sort[T ∈ Ordered] : List T → List T → Prop

evident sort xs ys
    length ys = length xs
    sorted ys
    permutation xs ys
```

Adding `permutation xs ys` intersects with the set of pairs where `ys` contains the same
elements as `xs`. The three conditions together uniquely identify the sorted permutation.

We need `permutation`. Let's define it:

```evident
claim permutation[T ∈ Eq] : List T → List T → Prop

evident permutation [] []
evident permutation [x | xs] ys
    member x ys
    permutation xs (remove_one x ys)
```

And supporting claims:

```evident
claim member[T ∈ Eq] : T → List T → semidet
claim remove_one[T ∈ Eq] : T → List T → List T → det

evident member x [x | _]
evident member x [_ | rest]
    member x rest

evident remove_one x [x | rest] rest
evident remove_one x [y | rest] [y | result] when x ≠ y
    remove_one x rest result
```

Now the query:

```evident
? sort [3, 1, 2] ?result
```

```
-- Solver returns:
result = [1, 2, 3]        ✓ unique solution

-- The three constraints together uniquely determine the answer:
-- • same length as input ✓
-- • non-decreasing order ✓
-- • same elements (permutation) ✓
-- There is exactly one list satisfying all three. The solver finds it.
```

---

## The key insight

We never wrote a sorting algorithm. `sort` names the intersection of three sets:

- `{ (xs, ys) | length xs = length ys }`
- `{ (xs, ys) | ys is sorted }`
- `{ (xs, ys) | ys is a permutation of xs }`

Any pair satisfying all three conditions is the unique sorted permutation of the input.
The solver's job is to find an element in that intersection — a witness that belongs to
all three sets simultaneously.

The solver can use any strategy: constraint propagation, search, backtracking. For small lists
it might just try permutations. For large lists it would need more structure. We could provide
search hints or additional redundant constraints to guide it.

---

## Step 4: Making it parametric and reusable

The definitions above already use type parameters. Let's see them composed:

```evident
-- A claim that the maximum element of a list is some value
claim list_max[T ∈ Ordered] : List T → T → semidet

evident list_max [x] x
evident list_max [x | rest] m
    list_max rest m_rest
    m = max x m_rest

claim max[T ∈ Ordered] : T → T → T → det

evident max a b a when a ≥ b
evident max a b b when b > a
```

Now we can compose: a sorted list's last element is its maximum.

```evident
claim last[T] : List T → T → semidet

evident last [x] x
evident last [_ | rest] x
    last rest x

-- Composition: the last element of a sorted list is the maximum
sorted_list_last_is_max : sorted xs, last xs m ⇒ list_max xs m
```

This reads: if `sorted xs` is established and `last xs m` is established,
then `list_max xs m` is also established. This is a forward implication —
a derived fact, not a definition.

---

## Step 5: Dependent types — constraining the output's type

We can make the output type dependent on the input:

```evident
-- A sorted list of the same length as the input
type SortedOf[T ∈ Ordered] xs = { ys ∈ List T | sorted ys, permutation xs ys }

claim sort[T ∈ Ordered] : (xs ∈ List T) → SortedOf[T] xs → Prop
```

Now `sort xs ys` is not just claiming a relationship — `ys`'s type is the type
"a sorted permutation of xs". The type carries the specification. Note that
`SortedOf[T] xs` is the same set that step 3 defines via three `evident` conditions;
here it is simply given an explicit name as a type, written directly in set-builder
notation.

```evident
? sort [3, 1, 2] ?result
-- result : SortedOf[Nat] [3, 1, 2]
-- result = [1, 2, 3]   ✓
```

The type of `result` proves it is correct. An expression of type `SortedOf[Nat] [3, 1, 2]`
cannot be anything other than `[1, 2, 3]`. The type is the proof.
