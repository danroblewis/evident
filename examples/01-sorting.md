# Example 1: Sorting — Constraint Accumulation

This example shows the core Evident workflow: start with an underconstrained model, observe
what the solver produces (wrong), add constraints, observe improvement, repeat until correct.

We are not writing a sorting algorithm. We are writing a specification of what it means for
a list to be sorted, and the solver finds a sorted version.

---

## Step 0: The claim with no body

```evident
claim sort[T : Ordered] : List T -> List T -> Prop
```

We've declared that `sort` relates two lists. We haven't said anything about what that
relationship means. The solver has complete freedom.

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

## Step 1: The output must have the same length

```evident
claim sort[T : Ordered] : List T -> List T -> Prop

evident sort xs ys
    length ys = length xs
```

Now we've said `ys` must have the same number of elements as `xs`.

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

## Step 2: The output must be in sorted order

```evident
claim sort[T : Ordered] : List T -> List T -> Prop

evident sort xs ys
    length ys = length xs
    sorted ys
```

We need `sorted` to exist. Let's define it:

```evident
claim sorted[T : Ordered] : List T -> Prop

evident sorted []
evident sorted [_]
evident sorted [a, b | rest] when a <= b
    sorted [b | rest]
```

Now the solver knows `ys` must be non-decreasing.

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

## Step 3: The output must contain the same elements

```evident
claim sort[T : Ordered] : List T -> List T -> Prop

evident sort xs ys
    length ys = length xs
    sorted ys
    permutation xs ys
```

We need `permutation`. Let's define it:

```evident
claim permutation[T : Eq] : List T -> List T -> Prop

evident permutation [] []
evident permutation [x | xs] ys
    member x ys
    permutation xs (remove_one x ys)
```

And supporting claims:

```evident
claim member[T : Eq] : T -> List T -> semidet
claim remove_one[T : Eq] : T -> List T -> List T -> det

evident member x [x | _]
evident member x [_ | rest]
    member x rest

evident remove_one x [x | rest] rest
evident remove_one x [y | rest] [y | result] when x != y
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

We never wrote a sorting algorithm. The constraints define sorting:
a sorted version of a list is the unique permutation of that list that is in non-decreasing order.
The solver's job is to find the assignment that satisfies all constraints simultaneously.

The solver can use any strategy: constraint propagation, search, backtracking. For small lists
it might just try permutations. For large lists it would need more structure. We could provide
search hints or additional redundant constraints to guide it.

---

## Step 4: Making it parametric and reusable

The definitions above already use type parameters. Let's see them composed:

```evident
-- A claim that the maximum element of a list is some value
claim list_max[T : Ordered] : List T -> T -> semidet

evident list_max [x] x
evident list_max [x | rest] m
    list_max rest m_rest
    m = max x m_rest

claim max[T : Ordered] : T -> T -> T -> det

evident max a b a when a >= b
evident max a b b when b > a
```

Now we can compose: a sorted list's last element is its maximum.

```evident
claim last[T] : List T -> T -> semidet

evident last [x] x
evident last [_ | rest] x
    last rest x

-- Composition: the last element of a sorted list is the maximum
sorted_list_last_is_max : sorted xs, last xs m => list_max xs m
```

This reads: if `sorted xs` is established and `last xs m` is established,
then `list_max xs m` is also established. This is a forward implication —
a derived fact, not a definition.

---

## Step 5: Dependent types — constraining the output's type

We can make the output type dependent on the input:

```evident
-- A sorted list of the same length as the input
type SortedOf[T : Ordered] xs = { ys : List T | sorted ys, permutation xs ys }

claim sort[T : Ordered] : (xs : List T) -> SortedOf[T] xs -> Prop
```

Now `sort xs ys` is not just claiming a relationship — `ys`'s type is the type
"a sorted permutation of xs". The type carries the specification.

```evident
? sort [3, 1, 2] ?result
-- result : SortedOf[Nat] [3, 1, 2]
-- result = [1, 2, 3]   ✓
```

The type of `result` proves it is correct. An expression of type `SortedOf[Nat] [3, 1, 2]`
cannot be anything other than `[1, 2, 3]`. The type is the proof.
