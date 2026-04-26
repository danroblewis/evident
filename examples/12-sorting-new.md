# Example 12: Sorting

```evident
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
    xs ∈ List T
    lo ∈ T
    hi ∈ T
    minimum_of xs lo
    maximum_of xs hi


sorted xs, first_of xs m ⇒ minimum_of xs m
sorted xs, last_of  xs m ⇒ maximum_of xs m


∃ result ∈ SortedOf[Nat] [3, 1, 2]
-- result = [1, 2, 3]
```
