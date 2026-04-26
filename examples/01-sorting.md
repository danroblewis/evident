# Example 1: Sorting — Constraint Accumulation

Each step adds a membership condition to a set, intersecting it with a smaller collection.
We stop when the set contains exactly what we want.

We are not writing a sorting algorithm. We are writing a specification of what it means
for a list to be sorted, and the solver finds a member of that set.

---

## The sets we need

### `sorted` — the set of non-decreasing lists

```evident
claim sorted[T ∈ Ordered] : List T → Prop

evident sorted []
evident sorted [_]
evident sorted [a, b | rest] when a ≤ b
    sorted [b | rest]
```

`sorted` names a set. `sorted [1, 2, 3]` is a membership claim: `[1, 2, 3] ∈ sorted`.

### `occurrences` — how many times an element appears in a list

```evident
claim occurrences[T ∈ Eq] : T → List T → Nat → det

evident occurrences x [] 0

evident occurrences x [x | rest] n
    ∃ n0 : occurrences x rest n0
    n = n0 + 1

evident occurrences x [y | rest] n when x ≠ y
    occurrences x rest n
```

`occurrences x list n` is established when x appears exactly n times in list. The `∃ n0`
introduces the intermediate count as a witness — no bare unbound names.

Note: `member x list` would be `x ∈ list`. That is already first-class syntax.
No claim needed.

### `permutation` — same elements, same counts

```evident
claim permutation[T ∈ Eq] : List T → List T → Prop

evident permutation xs ys
    length xs = length ys
    ∀ x ∈ xs : occurrences x xs = occurrences x ys
```

`permutation xs ys` is established when `ys` contains all the same elements as `xs`
with the same multiplicity. This is a declarative characterisation — two constraints,
not a recursive procedure. No `remove_one`, no structural recursion over the list.

If `length xs = length ys` and every element of `xs` appears the same number of times
in `ys`, the lengths guarantee no extra elements sneak in.

---

## `SortedOf` — the set this whole example is about

```evident
type SortedOf[T ∈ Ordered] xs = { ys ∈ List T | sorted ys, permutation xs ys }
```

`SortedOf xs` is the set of all sorted permutations of `xs`. For any finite list with
a total ordering, this set contains exactly one element.

The constraint accumulation builds up this type definition step by step.

---

## Step 0: Naming the set — no membership conditions yet

```evident
type SortedOf[T ∈ Ordered] xs = { ys ∈ List T }
```

With no conditions, `SortedOf xs` is just `List T` — every list of the right type is a member.

```evident
? ∃ result ∈ SortedOf[Nat] [3, 1, 2]
```

```
-- Solver may return:
result = []           -- valid (List Nat)
result = [0, 0, 0]   -- valid (List Nat)
result = [999]        -- valid (List Nat)

-- Any List Nat qualifies. Useless.
```

---

## Step 1: First intersection — members of equal length

```evident
type SortedOf[T ∈ Ordered] xs = { ys ∈ List T | length ys = length xs }
```

```evident
? ∃ result ∈ SortedOf[Nat] [3, 1, 2]
```

```
-- Solver may return:
result = [0, 0, 0]   -- valid (length 3)
result = [9, 9, 9]   -- valid (length 3)
result = [3, 1, 2]   -- valid (length 3)

-- Right size. Wrong elements, wrong order.
```

---

## Step 2: Second intersection — members that are sorted

```evident
type SortedOf[T ∈ Ordered] xs = { ys ∈ List T | length ys = length xs, sorted ys }
```

```evident
? ∃ result ∈ SortedOf[Nat] [3, 1, 2]
```

```
-- Solver may return:
result = [0, 0, 0]   -- valid (length 3, sorted)
result = [1, 1, 1]   -- valid (length 3, sorted)
result = [1, 2, 4]   -- valid (length 3, sorted)  ← wrong elements

-- Sorted and right size. But still wrong elements.
```

---

## Step 3: Third intersection — members that are permutations of the input

```evident
type SortedOf[T ∈ Ordered] xs = { ys ∈ List T | sorted ys, permutation xs ys }
```

The length condition is subsumed by `permutation` (which already requires equal length),
so we drop it.

```evident
? ∃ result ∈ SortedOf[Nat] [3, 1, 2]
```

```
-- Solver returns:
result = [1, 2, 3]   ✓ unique solution

-- sorted ✓  same elements ✓  same length ✓
-- Exactly one list satisfies all conditions. The solver finds it.
```

---

## The key insight

`SortedOf xs` is the intersection of two sets:

- `{ ys | sorted ys }` — non-decreasing lists
- `{ ys | permutation xs ys }` — lists containing the same elements as xs

Any member of both is the unique sorted permutation of xs. The solver's job is to find
a witness — an element belonging to both sets simultaneously.

---

## Extraction: how to use the result

The `∃` is the extraction mechanism. The witness is a name available in subsequent lines:

```evident
evident process_data xs
    ∃ sorted_xs ∈ SortedOf xs
    first_element sorted_xs ?min    -- sorted_xs is now a bound name
    last_element  sorted_xs ?max
    range = max - min
```

No separate "sort function call." No `?result` as an output variable. You declare that
a sorted version of `xs` must exist, name it, and use the name below. If no such member
exists (e.g. the solver can't find one), the enclosing claim is not established.

---

## Composability: `SortedOf` used in other claims

```evident
-- The minimum of a list is the first element of its sorted form
claim minimum_of[T ∈ Ordered] : List T → T → semidet

evident minimum_of xs m
    ∃ sorted_xs ∈ SortedOf xs
    first_element sorted_xs m

-- The maximum of a list is the last element of its sorted form
claim maximum_of[T ∈ Ordered] : List T → T → semidet

evident maximum_of xs m
    ∃ sorted_xs ∈ SortedOf xs
    last_element sorted_xs m
```

Supporting claims:

```evident
claim first_element[T] : List T → T → semidet
claim last_element[T]  : List T → T → semidet

evident first_element [x | _] x

evident last_element [x] x
evident last_element [_ | rest] x
    last_element rest x
```

The forward implication connecting `sorted` to `minimum_of` and `maximum_of` is derivable
from the definitions — a sorted list's first element is necessarily the minimum, its last
the maximum:

```evident
∃ s ∈ SortedOf xs, first_element s m ⇒ minimum_of xs m
∃ s ∈ SortedOf xs, last_element  s m ⇒ maximum_of xs m
```

---

## Parametric reuse

`SortedOf` works for any type with a total ordering:

```evident
? ∃ result ∈ SortedOf[String] ["banana", "apple", "cherry"]
-- result = ["apple", "banana", "cherry"]  ✓

? ∃ result ∈ SortedOf[Nat] []
-- result = []  ✓  (empty list is its own sorted permutation)

? ∃ result ∈ SortedOf[Nat] [5]
-- result = [5]  ✓
```

The type parameter `[T ∈ Ordered]` constrains `T` to be a member of the `Ordered` set —
the set of types with a total ordering. Any such type gets `SortedOf` for free.
