# Example 13: Building the Sorted Trait From Primitives

Goal: define `in_order` as a reusable constraint trait that can be applied
to any parent claim with a sequence variable. No function application. No
injective relations. Only sets, pairs, and arithmetic.

---

## What is a List? Building it as a constraint system.

A list is not a primitive function. It is a **set of (position, value) pairs**
with the constraint that every valid position has exactly one value.

```evident
type Array T = {
    n       ∈ Nat
    entries ∈ Set (Nat, T)

    -- every valid position appears exactly once
    ∀ i ∈ {0..n-1} : exactly 1 { (j, v) ∈ entries | j = i }

    -- no out-of-range positions
    ∀ (i, _) ∈ entries : i ∈ {0..n-1}
}
```

`arr.entries` is the data. `arr.n` is the length.
There is no `arr[i]` — that would be function application.
Instead, "the value at position i" is: the unique `v` such that `(i, v) ∈ arr.entries`.

---

## Primitives assumed

Only these are taken as given:

```evident
{a..b}          -- the set of integers from a to b inclusive
Nat             -- natural numbers with arithmetic (+, -, <, =, ...)
T ∈ Ordered     -- T has a total ordering ≤
```

Everything else is built from sets, membership, and the constraints above.

---

## The in_order trait — no indexing needed

Consecutive positions differ by 1. We express "consecutive values are in order"
as a constraint over pairs of entries whose positions differ by exactly 1:

```evident
claim in_order[T ∈ Ordered]
    arr ∈ Array T
    ∀ (i, v1) ∈ arr.entries, (j, v2) ∈ arr.entries :
        j = i + 1 ⇒ v1 ≤ v2
```

No `arr[i]`. No function application. Just: for any two entries where one
position immediately follows the other, the values must be non-decreasing.

Empty arrays (`n = 0`): `entries = {}`, no pairs to compare, holds vacuously.
Single-element arrays (`n = 1`): no two entries with `j = i + 1`, holds vacuously.

---

## Applying the trait to a parent claim

`in_order` is now a standalone claim — a trait. Apply it to any parent claim
that has an `Array`-valued variable:

```evident
claim valid_event_log
    events ∈ Array Event

    in_order_by events .timestamp    -- events must be in chronological order
    all_events_recorded events       -- all expected events are present
    no_duplicate_events events       -- no event appears twice
```

`events` stays typed as `Array Event`. The `in_order_by` claim is attached
to it — not embedded in its type.

---

## in_order_by — generalised over a field

`in_order` checks the values directly. `in_order_by` checks a field of the values:

```evident
claim in_order_by[T, K ∈ Ordered]
    arr   ∈ Array T
    field ∈ String          -- the name of the field to order by
    ∀ (i, v1) ∈ arr.entries, (j, v2) ∈ arr.entries :
        j = i + 1 ⇒ v1.field ≤ v2.field
```

---

## The OrderedArray type — optional, for when the type should carry the guarantee

If you want the constraint enforced at the type level (can't construct an
unordered instance), embed it in a type:

```evident
type OrderedArray[T ∈ Ordered] = {
    n       ∈ Nat
    entries ∈ Set (Nat, T)

    -- Array constraints:
    ∀ i ∈ {0..n-1} : exactly 1 { (j, v) ∈ entries | j = i }
    ∀ (i, _) ∈ entries : i ∈ {0..n-1}

    -- Sorted constraint (the in_order trait, inlined):
    ∀ (i, v1) ∈ entries, (j, v2) ∈ entries :
        j = i + 1 ⇒ v1 ≤ v2
}
```

Or using pass-through to lift `in_order` into the type:

```evident
type OrderedArray[T ∈ Ordered] = {
    ..Array T
    ..in_order
}
```

`..Array T` lifts all of `Array T`'s variables and constraints into this type.
`..in_order` lifts the sortedness constraint and applies it to `entries` and `n`
already in scope.

---

## Summary: the dependency chain, no functions used

```
Nat, {a..b}, T ∈ Ordered, Set (Nat, T)
        ↓
Array T
    entries ∈ Set (Nat, T)
    every position has exactly one value
        ↓
in_order arr
    ∀ (i,v1),(j,v2) ∈ entries : j = i+1 ⇒ v1 ≤ v2
        ↓
in_order_by arr .field
    same, over v1.field and v2.field
        ↓
Applied to any parent claim:
    my_claim
        xs ∈ Array T
        in_order xs        -- the trait, attached
        other_constraint
```
