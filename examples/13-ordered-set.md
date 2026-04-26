# Example 13: Ordered Set — Building Up

Starting from scratch. We want a reusable constraint that can be attached
to any collection to assert it is in sorted order — a mixin, not a one-off.

---

## Step 1: What does "ordered" mean for a type?

`T ∈ Ordered` is a constraint on a type parameter meaning T has a total ordering —
every two elements can be compared with `≤`. We treat this as primitive for now.

```evident
-- T ∈ Ordered implies these hold for all a, b, c ∈ T:
--   a ≤ a                          (reflexive)
--   a ≤ b, b ≤ c ⇒ a ≤ c         (transitive)
--   a ≤ b, b ≤ a ⇒ a = b         (antisymmetric)
--   a ≤ b ∨ b ≤ a                 (total — every pair is comparable)
```

---

## Step 2: What does it mean for a sequence to be in order?

```evident
claim in_order[T ∈ Ordered]
    items ∈ List T
    ∀ (a, b) ∈ each_consecutive items : a ≤ b
```

This is the fundamental sorted-ness constraint. One statement. No recursion.
Works vacuously for empty and single-element lists.

---

## Step 3: An ordered list type

```evident
type OrderedList[T ∈ Ordered] = {
    items ∈ List T
    in_order items
}
```

`OrderedList T` is the set of all non-decreasing lists of T.
Any value of this type is guaranteed to be sorted.