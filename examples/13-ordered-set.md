# Example 13: Building the Sorted Trait From Primitives

Starting from scratch: what is a List, what primitives exist, and how do
we build a reusable sorted constraint from those primitives alone?

---

## What is a List?

A `List T` is a **function from positions to values**:

```evident
-- A list of n elements of type T is a function from {0..n-1} to T.
-- Positions are the natural numbers 0, 1, ..., n-1.
-- The position IS the order.

type List T = {
    length ∈ Nat
    at     ∈ {0..length-1} → T     -- 'at' maps each position to a value
}
```

`xs.at[i]` is the element at position i. We write `xs[i]` as shorthand.

A `Set T` has no `at` and no `length` — it has no positions, no indexing,
no natural order. You cannot sort a set because there is nothing to arrange.
Sets are equal when they contain the same elements regardless of any order.

---

## Primitives

These are the building blocks. We treat them as given.

```evident
-- Integer range: the set of natural numbers from a to b inclusive
-- {0..n-1} is the set of valid positions in a list of length n
{a..b} : Set Nat

-- Indexed access: xs[i] is the element of xs at position i
-- (shorthand for xs.at[i])
xs[i] : T    -- where xs : List T, i ∈ {0..length xs - 1}

-- Length: how many positions exist
length xs : Nat

-- Ordering: T ∈ Ordered means ≤ is defined over T
-- satisfying: reflexive, transitive, antisymmetric, total
T ∈ Ordered → (≤) : T → T → Bool
```

---

## Consecutive pairs — the key derived concept

Two elements are *consecutive* in a list when their positions differ by 1.
The set of all consecutive pairs is:

```evident
claim consecutive_pairs[T]
    xs    ∈ List T
    pairs ∈ Set (T, T)
    pairs = { (xs[i], xs[i+1]) | i ∈ {0..length xs - 2} }
```

For `xs = [3, 1, 4, 1]`:
- positions: `{0, 1, 2, 3}`
- valid i values: `{0, 1, 2}` (= `{0..length xs - 2}`)
- pairs: `{(3,1), (1,4), (4,1)}`

For `xs = []`: `{0..length xs - 2}` = `{0..-2}` = `{}` → pairs = `{}`
For `xs = [5]`: `{0..-1}` = `{}` → pairs = `{}`

The empty cases are handled by arithmetic on the range — no special cases needed.

---

## The sorted trait

```evident
claim in_order[T ∈ Ordered]
    xs ∈ List T
    ∀ i ∈ {0..length xs - 2} : xs[i] ≤ xs[i+1]
```

Or equivalently, using `consecutive_pairs`:

```evident
claim in_order[T ∈ Ordered]
    xs ∈ List T
    ∀ (a, b) ∈ consecutive_pairs xs : a ≤ b
```

Both say the same thing. The first is more primitive (directly over indices).
The second names the concept. `consecutive_pairs` earns its name by making
the intent legible — `∀ (a, b) ∈ consecutive_pairs xs` reads as "for every
adjacent pair in xs."

---

## An ordered list type

```evident
type OrderedList[T ∈ Ordered] = {
    xs ∈ List T
    in_order xs
}
```

Any value of type `OrderedList T` is guaranteed to satisfy `in_order`.
The type carries the constraint.

---

## Attaching the trait to other types

`in_order` is reusable — add it to any type that has a list-valued field:

```evident
-- A schedule where assignments are in chronological order
type ChronologicalSchedule = {
    assignments ∈ List Assignment
    in_order_by assignments .slot.start    -- order by start time
}

-- A leaderboard where scores are in descending order
type Leaderboard = {
    entries ∈ List { player ∈ String, score ∈ Nat }
    in_order_by entries .score descending
}
```

`in_order_by xs .field` is `in_order` applied to a field projection:

```evident
claim in_order_by[T, K ∈ Ordered]
    xs    ∈ List T
    field ∈ T → K
    ∀ i ∈ {0..length xs - 2} : field xs[i] ≤ field xs[i+1]
```

---

## Summary: the full dependency chain

```
T ∈ Ordered          -- element type has ≤
{a..b}               -- range as a set of positions
xs[i]                -- indexed access
length xs            -- size of the position set
        ↓
consecutive_pairs xs -- { (xs[i], xs[i+1]) | i ∈ {0..length xs - 2} }
        ↓
in_order xs          -- ∀ (a,b) ∈ consecutive_pairs xs : a ≤ b
        ↓
OrderedList T        -- type that carries in_order as a constraint
in_order_by xs .f    -- generalisation over a field projection
```
