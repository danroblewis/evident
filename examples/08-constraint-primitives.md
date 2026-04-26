# Example 8: Constraint Primitives

The most common constraint programming primitives, implemented in Evident itself.
These would ship as a standard library. A few built-ins are assumed:
`|S|` (cardinality of a set), `Σ S` (sum of a numeric set), and arithmetic operators.

---

```evident
-- ── Distinctness ─────────────────────────────────────────────────────────────

-- All elements of a collection are distinct.
-- The set comprehension deduplicates; if any element repeated, |set| < |list|.
claim all_different[T ∈ Eq]
    items ∈ List T
    |{ x | x ∈ items }| = |items|


-- ── Counting ─────────────────────────────────────────────────────────────────

-- How many elements of a collection satisfy a predicate.
claim count[T]
    items ∈ Collection T
    pred  ∈ T → Prop
    n     ∈ Nat
    n = |{ x ∈ items | pred x }|

-- Shorthand cardinality constraints built on count.
claim at_most[T]
    n     ∈ Nat
    items ∈ Collection T
    pred  ∈ T → Prop
    |{ x ∈ items | pred x }| ≤ n

claim at_least[T]
    n     ∈ Nat
    items ∈ Collection T
    pred  ∈ T → Prop
    |{ x ∈ items | pred x }| ≥ n

claim exactly[T]
    n     ∈ Nat
    items ∈ Collection T
    pred  ∈ T → Prop
    |{ x ∈ items | pred x }| = n


-- ── Aggregation ──────────────────────────────────────────────────────────────

claim sum_of
    items ∈ Collection Nat
    total ∈ Nat
    total = Σ { x | x ∈ items }

claim max_of[T ∈ Ordered]
    items ∈ Collection T
    m     ∈ T
    m ∈ items
    ∀ x ∈ items : x ≤ m

claim min_of[T ∈ Ordered]
    items ∈ Collection T
    m     ∈ T
    m ∈ items
    ∀ x ∈ items : m ≤ x


-- ── Bounds ───────────────────────────────────────────────────────────────────

claim within_range[T ∈ Ordered]
    value ∈ T
    lower ∈ T
    upper ∈ T
    lower ≤ value
    value ≤ upper


-- ── Ordering ─────────────────────────────────────────────────────────────────

claim increasing[T ∈ Ordered]
    items ∈ List T
    ∀ (a, b) ∈ each_consecutive items : a ≤ b

claim strictly_increasing[T ∈ Ordered]
    items ∈ List T
    ∀ (a, b) ∈ each_consecutive items : a < b

claim decreasing[T ∈ Ordered]
    items ∈ List T
    ∀ (a, b) ∈ each_consecutive items : a ≥ b


-- ── Set structure ─────────────────────────────────────────────────────────────

-- Every item appears in exactly one group; groups cover all items; no extras.
claim partition[T]
    items  ∈ Set T
    groups ∈ Set (Set T)
    ∀ x ∈ items : exactly 1 groups (g → x ∈ g)
    ∀ g ∈ groups : g ⊆ items

-- One set is a subset of another.
-- (⊆ is primitive, but defined here for documentation)
claim subset_of[T]
    a ∈ Set T
    b ∈ Set T
    ∀ x ∈ a : x ∈ b


-- ── Usage examples ────────────────────────────────────────────────────────────

-- Rewrite no_double_assignment from example 07 using all_different:
claim no_double_assignment
    assignments ∈ Set Assignment
    all_different { a.person | a ∈ assignments }

-- A valid sudoku row: 9 cells, values 1–9, all different.
claim valid_sudoku_row
    cells ∈ List Nat
    |cells| = 9
    ∀ x ∈ cells : within_range x 1 9
    all_different cells

-- A budget constraint using sum_of:
claim team_within_budget
    assignments ∈ Set Assignment
    budget      ∈ Nat
    sum_of { a.person.salary | a ∈ assignments } total
    total ≤ budget

-- Find the highest-paid person on a team:
claim highest_salary
    assignments ∈ Set Assignment
    person      ∈ Person
    person ∈ { a.person | a ∈ assignments }
    max_of { a.person.salary | a ∈ assignments } person.salary
```
