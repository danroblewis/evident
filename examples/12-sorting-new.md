# Example 12: Sorting — Three Formulations

---

## Version 1: Flat — independent claims, general utilities

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

sorted xs, first_of ys m ⇒ minimum_of xs m
sorted xs, last_of  ys m ⇒ maximum_of xs m

∃ result ∈ SortedOf[Nat] [3, 1, 2]
-- result = [1, 2, 3]
```

---

## Version 2: Nested — parent claim groups the pair and derived properties

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

claim list_sorted[T ∈ Ordered]
    xs ∈ List T
    ys ∈ SortedOf[T] xs

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

assert xs = [3, 1, 2]
assert ys ∈ List Nat

list_sorted
list_sorted.minimum     -- min = 1
list_sorted.maximum     -- max = 3
list_sorted.range       -- lo = 1, hi = 3
```

---

## Version 3: Claims embedded in the type — open question

What if the derived claims live inside the type itself, and accessing them
via dot notation on a type instance gives you a constraint sub-system?

```evident
type SortedList[T ∈ Ordered] = {
    xs ∈ List T
    ys ∈ List T
    sorted ys
    permutation xs ys

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
}

assert s ∈ SortedList[Nat]
s.xs = [3, 1, 2]        -- constrain xs; solver fills in ys = [1, 2, 3]

s.minimum               -- constraint sub-system: min is unbound, solver fills it in → 1
s.maximum               -- → 3
s.range                 -- lo and hi unbound → lo = 1, hi = 3
```

`s.minimum` is not a method call and does not "return" a value. It is a
constraint sub-system — the `minimum` claim applied to the values in `s`,
with `min` left unbound for the solver to determine.

The question: what does it mean for a type to carry claims? How does
`s.minimum` differ from `list_sorted.minimum`? What are the rules for
which variables are "in scope" inside a type-embedded claim?
