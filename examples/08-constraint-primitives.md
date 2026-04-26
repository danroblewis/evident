# Example 8: Constraint Primitives

The most common constraint programming primitives, implemented in Evident itself.
These would ship as a standard library. A few built-ins are assumed:
`|S|` (cardinality of a set), `Σ S` (sum of a numeric set), and arithmetic operators.

Filtering is done at the call site using set comprehensions — no higher-order
predicate parameters needed.

---

```evident
-- ── Distinctness ─────────────────────────────────────────────────────────────

-- All elements of a collection are distinct.
-- The set comprehension deduplicates; if any element repeated, |set| < |list|.
claim all_different[T ∈ Eq]
    items ∈ List T
    |{ x | x ∈ items }| = |items|


-- ── Counting ─────────────────────────────────────────────────────────────────

-- Cardinality of a (possibly filtered) set.
-- Callers pass a set comprehension: count { x ∈ items | x > 5 } n
claim count[T]
    items ∈ Set T
    n     ∈ Nat
    n = |items|

-- Usage: pass a filtered set at the call site.
-- at_most 3 { x ∈ items | x > 5 }
claim at_most[T]
    n     ∈ Nat
    items ∈ Set T
    |items| ≤ n

claim at_least[T]
    n     ∈ Nat
    items ∈ Set T
    |items| ≥ n

claim exactly[T]
    n     ∈ Nat
    items ∈ Set T
    |items| = n


-- ── Aggregation ──────────────────────────────────────────────────────────────

claim sum_of
    items ∈ Set Nat
    total ∈ Nat
    total = Σ items

claim max_of[T ∈ Ordered]
    items ∈ Set T
    m     ∈ T
    m ∈ items
    ∀ x ∈ items : x ≤ m

claim min_of[T ∈ Ordered]
    items ∈ Set T
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

-- Every item appears in exactly one group; groups cover all items.
claim partition[T]
    items  ∈ Set T
    groups ∈ Set (Set T)
    ∀ x ∈ items : exactly 1 { g ∈ groups | x ∈ g }
    ∀ g ∈ groups : g ⊆ items


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

-- Budget constraint: sum of salaries within budget.
claim team_within_budget
    assignments ∈ Set Assignment
    budget      ∈ Nat
    sum_of { a.person.salary | a ∈ assignments } total
    total ≤ budget

-- How many assignments involve senior engineers?
claim senior_count
    assignments ∈ Set Assignment
    n           ∈ Nat
    count { a ∈ assignments | a.person.level = "senior" } n
```
