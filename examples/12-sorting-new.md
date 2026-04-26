# Example 12: Sorting

```evident
-- ── General utilities (reusable outside this context) ────────────────────────

type SortedOf[T ∈ Ordered] xs = {
    ys ∈ List T
    sorted ys
    permutation xs ys
}

claim sorted[T ∈ Ordered]
    list ∈ List T
    ∀ (a, b) ∈ each_consecutive list : a ≤ b

claim occurrences[T ∈ Eq]
    x    ∈ T
    list ∈ List T
    n    ∈ Nat
    n = count list[. = x]

claim permutation[T ∈ Eq]
    xs ∈ List T
    ys ∈ List T
    length xs = length ys
    ∀ x ∈ xs : occurrences x xs = occurrences x ys


-- ── Parent claim: a list and its sorted form, with derived properties ─────────

claim list_sorted[T ∈ Ordered]
    xs ∈ List T
    ys ∈ SortedOf[T] xs     -- ys is constrained by type; solver fills it in

    claim minimum
        min ∈ T
        first_of ys = min

    claim maximum
        max ∈ T
        last_of ys = max

    claim range
        lo ∈ T
        hi ∈ T
        minimum lo
        maximum hi


first_of ys m ⇒ list_sorted.minimum m
last_of  ys m ⇒ list_sorted.maximum m


-- ── Usage ─────────────────────────────────────────────────────────────────────

assert xs = [3, 1, 2]
assert ys ∈ List Nat        -- unbound: solver fills this in

list_sorted                 -- establishes ys = [1, 2, 3]
list_sorted.minimum         -- min = 1
list_sorted.maximum         -- max = 3
list_sorted.range           -- lo = 1, hi = 3
```
