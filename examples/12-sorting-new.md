# Example 12: Sorting — Rewritten with Current Patterns

Same problem as example 01, rewritten using the patterns we've settled on:
- `SortedOf` as a **refined type** (type carrying its own constraints)
- `sorted` as a **single universal statement** — no multiple clauses, no base cases
- `occurrences` using the **filter sugar** — one declarative line
- Extraction via **`∃ result ∈ SortedOf xs`** — no `?result` output variable

---

## The refined type

`SortedOf xs` is the set of all sorted permutations of `xs`. It carries its own
constraints — a type that knows what makes a value valid.

```evident
type SortedOf[T ∈ Ordered] xs = {
    ys ∈ List T
    sorted ys
    permutation xs ys
}
```

For any finite list with a total ordering, `SortedOf xs` contains exactly one element.

---

## The supporting claims

```evident
-- sorted: the set of non-decreasing lists.
-- No base cases — empty and singleton lists have no consecutive pairs,
-- so the ∀ holds vacuously. One statement covers all cases.
claim sorted[T ∈ Ordered]
    list ∈ List T
    ∀ (a, b) ∈ each_consecutive list : a ≤ b


-- occurrences: how many times x appears in list.
-- Uses filter sugar: list[. = x] is the multiset of elements equal to x.
claim occurrences[T ∈ Eq]
    x    ∈ T
    list ∈ List T
    n    ∈ Nat
    n = count list[. = x]


-- permutation: xs and ys contain the same elements with the same counts.
-- Declarative — no structural recursion, no remove_one.
claim permutation[T ∈ Eq]
    xs ∈ List T
    ys ∈ List T
    length xs = length ys
    ∀ x ∈ xs : occurrences x xs = occurrences x ys
```

---

## Extraction

The `∃` introduces a witness — the name `result` is available below.

```evident
∃ result ∈ SortedOf[Nat] [3, 1, 2]
-- result = [1, 2, 3]  ✓

∃ result ∈ SortedOf[Nat] []
-- result = []  ✓

∃ result ∈ SortedOf[String] ["banana", "apple", "cherry"]
-- result = ["apple", "banana", "cherry"]  ✓
```

`SortedOf[Nat] [3, 1, 2]` is the type `{[1, 2, 3]}` — a one-element set.
The `∃` names its only member.

---

## Composability

`SortedOf` composes into other claims without repeating the sorting logic.

```evident
claim minimum_of[T ∈ Ordered]
    xs  ∈ List T
    min ∈ T
    ∃ sorted_xs ∈ SortedOf xs
    first_of sorted_xs = min

claim maximum_of[T ∈ Ordered]
    xs  ∈ List T
    max ∈ T
    ∃ sorted_xs ∈ SortedOf xs
    last_of sorted_xs = max

claim range_of[T ∈ Ordered]
    xs  ∈ List T
    lo  ∈ T
    hi  ∈ T
    minimum_of xs lo
    maximum_of xs hi
```

Forward implications — derived facts that fire automatically:

```evident
sorted xs, first_of xs m ⇒ minimum_of xs m
sorted xs, last_of  xs m ⇒ maximum_of xs m
```

---

## What changed from example 01

| Old (01-sorting.md) | New (here) |
|---|---|
| 3 `evident` blocks for `sorted` (base cases + recursive) | 1 universal statement — base cases are vacuous |
| 3 `evident` blocks for `occurrences` (with `∃ n0 :`) | `count list[. = x]` — one line |
| `SortedOf` was a dependent type at the bottom | `SortedOf` is the central refined type at the top |
| `? sort [3, 1, 2] ?result` output variable syntax | `∃ result ∈ SortedOf xs` — extraction via type membership |
| `claim` + separate `evident` blocks | Flat constraint list under `claim` |
| `: Prop` and `: det` annotations | Dropped — no return type |
