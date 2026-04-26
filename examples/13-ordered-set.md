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

## consecutive_pairs — building the set first

The pattern `∀ (i,v1) ∈ entries, (j,v2) ∈ entries : j = i+1 ⇒ ...` would
appear constantly. Extract it: build the set of consecutive value pairs once,
then assert things about that set.

```evident
claim consecutive_pairs[T]
    arr   ∈ Indexable T
    pairs ∈ Set (T, T)
    pairs = { (v1, v2) | (i, v1) ∈ arr.entries, (i+1, v2) ∈ arr.entries }
```

Two generators. The `i+1` in the second generator does the adjacency work —
no explicit `j = i + 1` condition needed. `pairs` is the set of all
consecutive value pairs in the array.

Empty arrays: `entries = {}`, no pairs generated, `pairs = {}`.
Single-element arrays: no two entries with adjacent positions, `pairs = {}`.
Both hold vacuously — no special cases needed.

---

## The in_order trait — build the set, then assert

```evident
claim in_order[T ∈ Ordered]
    arr ∈ Indexable T
    ∀ (a, b) ∈ consecutive_pairs arr : a ≤ b
```

No `arr[i]`. No function application. No `j = i + 1`.
Build `consecutive_pairs arr`, assert `a ≤ b` over it.

Once `consecutive_pairs` exists, many sequential constraints follow the same shape:

```evident
claim strictly_in_order[T ∈ Ordered]
    arr ∈ Indexable T
    ∀ (a, b) ∈ consecutive_pairs arr : a < b

claim no_equal_adjacent[T ∈ Eq]
    arr ∈ Indexable T
    ∀ (a, b) ∈ consecutive_pairs arr : a ≠ b

claim bounded_step
    arr      ∈ Indexable Nat
    max_step ∈ Nat
    ∀ (a, b) ∈ consecutive_pairs arr : b - a ≤ max_step
```

All of them: build the set, assert the condition. The `j = i + 1` pattern
is written once inside `consecutive_pairs` and never again.

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
Indexable T (interface)
    n       ∈ Nat
    entries ∈ Set (Nat, T)
        ↓
Array T (implementation of Indexable T)
    adds: ∀ i ∈ {0..n-1} : exactly 1 value at position i
    adds: no out-of-range positions
        ↓
consecutive_pairs arr
    { (v1, v2) | (i,v1) ∈ entries, (i+1,v2) ∈ entries }
        ↓
in_order arr              -- ∀ (a,b) ∈ consecutive_pairs arr : a ≤ b
strictly_in_order arr     -- ∀ (a,b) ∈ consecutive_pairs arr : a < b
no_equal_adjacent arr     -- ∀ (a,b) ∈ consecutive_pairs arr : a ≠ b
bounded_step arr k        -- ∀ (a,b) ∈ consecutive_pairs arr : b-a ≤ k
        ↓
Applied to any parent claim:
    my_claim
        xs ∈ Indexable T
        in_order xs           -- the trait, attached
        other_constraint
```
