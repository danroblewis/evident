# Example 13: Ordered Set — The Tight Binding Pattern

The pattern: instead of writing complex expressions at every use site,
introduce a new variable and bind it tightly (= exactly one value).
The solver eliminates tightly-bound variables by substitution before search.
Use sites stay clean. Readability flows left to right.

---

## Primitives assumed

```evident
Nat                 -- natural numbers with arithmetic
{a..b}              -- the set of integers from a to b inclusive
T ∈ Ordered         -- T has a total ≤ ordering
A × B          -- sets of pairs
```

---

## Indexable T — with consecutive_pairs as a tight binding

```evident
type Indexable T = {
    n                 ∈ Nat
    entries           ⊆ Nat × T
    consecutive_pairs ⊆ T × T

    -- every valid index maps to exactly one value
    ∀ i ∈ {0..n-1} : exactly 1 { (j, v) ∈ entries | j = i }

    -- no out-of-range indices
    ∀ (i, _) ∈ entries : i ∈ {0..n-1}

    -- tight binding: consecutive_pairs is determined by entries
    consecutive_pairs = { (v1, v2) | (i, v1) ∈ entries, (i+1, v2) ∈ entries }
}
```

`consecutive_pairs` is a field like any other. The solver eliminates it by
substitution before search — tightly-bound variables are free.

---

## Sequential traits — all read left to right

```evident
claim in_order[T ∈ Ordered]
    arr ∈ Indexable T
    ∀ (a, b) ∈ arr.consecutive_pairs : a ≤ b

claim strictly_in_order[T ∈ Ordered]
    arr ∈ Indexable T
    ∀ (a, b) ∈ arr.consecutive_pairs : a < b

claim no_equal_adjacent[T ∈ Eq]
    arr ∈ Indexable T
    ∀ (a, b) ∈ arr.consecutive_pairs : a ≠ b

claim bounded_step
    arr      ∈ Indexable Nat
    max_step ∈ Nat
    ∀ (a, b) ∈ arr.consecutive_pairs : b - a ≤ max_step
```

Every trait: start with `arr`, reach `.consecutive_pairs`, assert the condition.
The set comprehension is written once inside `Indexable T`. Never again.

---

## Applying traits to a parent claim

```evident
claim valid_event_log
    events ∈ Indexable Event

    in_order events
    all_events_recorded events
    no_duplicate_events events
```

`in_order events` — apply the trait. `events` flows by names-match.
No intermediate variables. No set comprehensions at the call site.

---

## OrderedArray — a type that carries the guarantee

When you want the sorted constraint enforced at the type level:

```evident
type OrderedArray[T ∈ Ordered] = {
    ..Indexable T
    ∀ (a, b) ∈ consecutive_pairs : a ≤ b
}
```

`..Indexable T` lifts all of `Indexable T`'s variables and constraints —
including `consecutive_pairs` — into this type. The `∀` line then
uses `consecutive_pairs` directly since it is already in scope.

---

## Dependency chain

```
Nat, {a..b}, T ∈ Ordered, Nat × T
        ↓
Indexable T
    n                 ∈ Nat
    entries           ⊆ Nat × T
    consecutive_pairs ⊆ T × T       ← tight binding
        ↓
arr.consecutive_pairs                    ← field access, left to right
        ↓
in_order             ∀ (a,b) : a ≤ b
strictly_in_order    ∀ (a,b) : a < b
no_equal_adjacent    ∀ (a,b) : a ≠ b
bounded_step         ∀ (a,b) : b-a ≤ k
        ↓
OrderedArray[T]      carries in_order at the type level
```
