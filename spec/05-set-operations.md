# Evident Specification — Set Operations

Sets are the central data structure in Evident. Every claim names a set; every variable
ranges over a set; every constraint is a membership condition. This section covers the
syntax for constructing, transforming, and querying sets.

---

## Set literals

```evident
{}              -- the empty set
{1, 2, 3}       -- an enumerated set of values
{a, b, c}       -- an enumerated set of variables (or names in scope)
```

The empty set `{}` has type `Set T` for any `T`. Enumerated sets require all elements
to have the same type.

```evident
claim uses_fixed_set
    allowed ∈ Set Nat
    allowed = {1, 3, 5, 7, 9}   -- tight binding to a literal set
    ∀ x ∈ allowed : x ∈ {0..9}  -- every element is a single digit
```

---

## Set comprehension

Set comprehensions construct new sets by filtering or mapping existing ones.
The general form is `{ expression | generators, conditions }`.

### Filter: elements satisfying a predicate

```evident
{ x ∈ S | P(x) }       -- elements of S for which P holds
```

The result is a subset of `S`. All `x` in the result satisfy `P(x)`.

```evident
claim senior_engineers
    employees ∈ Set Person
    seniors   ∈ Set Person
    seniors = { e ∈ employees | e.level = "senior" }
```

### Map: applying a function to each element

```evident
{ f(x) | x ∈ S }       -- apply f to each element of S
```

The result has the type of `f(x)`. Elements of `S` that produce the same value
under `f` are merged (sets contain no duplicates).

```evident
claim task_names
    tasks ∈ Set Task
    names ∈ Set String
    names = { t.name | t ∈ tasks }
```

### Map with filter

```evident
{ f(x) | x ∈ S, P(x) }     -- apply f to elements of S satisfying P
```

Combining both forms: first restrict to elements satisfying `P`, then apply `f`.

```evident
claim senior_salaries
    employees ∈ Set Person
    salaries  ∈ Set Nat
    salaries = { e.salary | e ∈ employees, e.level = "senior" }
```

### Cross-product comprehension

```evident
{ (x, y) | x ∈ S, y ∈ T }     -- all pairs (x, y) where x ∈ S and y ∈ T
```

Multiple generators in a comprehension produce a cross product. Each combination
satisfying all conditions appears exactly once in the result.

```evident
claim all_pairs
    people ∈ Set Person
    pairs  ∈ Set (Person × Person)
    pairs = { (a, b) | a ∈ people, b ∈ people, a ≠ b }
```

Additional conditions after the generators further filter the cross product.

```evident
claim edges_from
    graph ∈ Graph
    src   ∈ Node
    out   ∈ Set (Node × Node)
    out = { (u, v) | (u, v) ∈ graph.edges, u = src }
```

---

## Field projection (on sets of records)

Field projection applies field access to every element of a set, producing the set
of all projected values. It is syntactic sugar for a map comprehension.

```evident
S.field             -- { a.field | a ∈ S }
S.field.subfield    -- { a.field.subfield | a ∈ S }  (chained projection)
S.0                 -- { a.0 | a ∈ S }  (first component of each tuple)
S.1                 -- { a.1 | a ∈ S }  (second component of each tuple)
```

Projection is particularly useful for extracting one dimension of a relation or
record set without writing an explicit comprehension.

```evident
claim team_names
    team  ∈ Set Person
    names ∈ Set String
    names = team.name           -- equivalent to { p.name | p ∈ team }

claim assigned_rooms
    schedule ∈ Set (Talk × Room)
    rooms    ∈ Set Room
    rooms = schedule.1          -- second element of each pair
```

Chained projection traverses nested records:

```evident
claim dept_cities
    employees ∈ Set Person
    cities    ∈ Set String
    cities = employees.department.location.city
    -- equivalent to { e.department.location.city | e ∈ employees }
```

---

## Filter sugar

Filter sugar applies a membership condition to a set inline, without introducing
an explicit comprehension variable. The current element is referred to with `.`.

```evident
S[condition]            -- { a ∈ S | condition }  where . refers to a
S[.field = value]       -- elements of S whose .field equals value
S[.field > 5]           -- elements of S whose .field exceeds 5
```

Filters chain: each application further restricts the set.

```evident
S[.field = v1][.other = v2]   -- elements satisfying both conditions
```

Filter sugar and field projection compose naturally:

```evident
claim active_senior_salaries
    employees ∈ Set Person
    salaries  ∈ Set Nat
    salaries = employees[.level = "senior"][.active = true].salary
    -- equivalent to { e.salary | e ∈ employees, e.level = "senior", e.active = true }
```

The `.` shorthand refers to the element being tested. It is only available inside
`[...]` filter expressions, not in general claim bodies.

---

## Set algebra

Standard mathematical set operations. All operate on sets of the same element type.

```evident
S ∪ T       -- union: elements in S, T, or both
S ∩ T       -- intersection: elements in both S and T
S \ T       -- difference: elements in S that are not in T
S × T       -- Cartesian product: all pairs (s, t) where s ∈ S and t ∈ T
|S|         -- cardinality: number of elements in S (a Nat)
```

```evident
claim merged_teams
    team_a   ∈ Set Person
    team_b   ∈ Set Person
    combined ∈ Set Person
    combined = team_a ∪ team_b

claim shared_availability
    alice_free ∈ Set Slot
    bob_free   ∈ Set Slot
    overlap    ∈ Set Slot
    overlap = alice_free ∩ bob_free

claim unassigned_tasks
    all_tasks    ∈ Set Task
    assigned     ∈ Set Task
    unassigned   ∈ Set Task
    unassigned = all_tasks \ assigned

claim candidate_pairs
    applicants ∈ Set Person
    roles      ∈ Set Role
    candidates ∈ Set (Person × Role)
    candidates = applicants × roles
```

Cardinality `|S|` is a term that produces a `Nat`. It appears in arithmetic expressions
and constraints, not as a standalone statement.

```evident
claim balanced_teams
    team_a ∈ Set Person
    team_b ∈ Set Person
    |team_a| = |team_b|

claim majority_voted
    voters    ∈ Set Person
    yes_votes ∈ Set Person
    yes_votes ⊆ voters
    |yes_votes| * 2 > |voters|    -- strict majority
```

---

## Grouping

Grouping partitions a set into subsets sharing a common field value.

```evident
S grouped_by .field     -- partition S; elements with equal .field go together
```

The result is a set of sets. Each inner set contains all elements of `S` with the same
value for `.field`. Used with `∀` to apply a condition to each group independently.

```evident
∀ group ∈ S grouped_by .field : condition_on group
```

```evident
claim tasks_balanced_by_day
    schedule ∈ Set Assignment
    ∀ group ∈ schedule grouped_by .day : |group| ≤ 5

claim each_dept_has_manager
    employees ∈ Set Person
    ∀ dept ∈ employees grouped_by .department :
        ∃ p ∈ dept : p.role = "manager"

claim no_room_double_booked
    bookings ∈ Set Booking
    ∀ group ∈ bookings grouped_by .room :
        ∀ a, b ∈ group : a ≠ b ⇒ ¬ overlaps a.slot b.slot
```

`grouped_by` does not appear in claim heads — it is a body-level set operation used
to structure a universal quantification.

---

## Ranges

Integer ranges produce sets of consecutive integers.

```evident
{a..b}      -- the set of integers from a to b inclusive: {a, a+1, ..., b}
{0..n-1}    -- standard index range for a sequence of length n
```

Ranges are closed on both ends. If `a > b`, the result is the empty set `{}`.

```evident
claim valid_indices
    arr     ∈ Indexable T
    indices ∈ Set Nat
    indices = {0..arr.n - 1}

claim sum_first_hundred
    total ∈ Nat
    total = Σ {1..100}

claim all_scores_valid
    scores ∈ Set Nat
    ∀ s ∈ scores : s ∈ {0..100}
```

Ranges interact naturally with `|S|`:

```evident
claim dense_assignment
    entries ∈ Set (Nat × T)
    n       ∈ Nat
    entries.0 = {0..n-1}    -- every index from 0 to n-1 is used
```

---

## Tuple operations

Tuples are ordered, fixed-length products. They are the element type of Cartesian
products and the entry type of indexed structures.

### Tuple literals

```evident
(a, b)          -- a pair; has type A × B
(a, b, c)       -- a triple; has type A × B × C
```

Tuples are value types. Two tuples are equal if and only if all their components are equal.

```evident
claim edge_exists
    graph ∈ Graph
    u     ∈ Node
    v     ∈ Node
    (u, v) ∈ graph.edges    -- membership test on a set of pairs
```

### Component access

```evident
t.0     -- first component of tuple t
t.1     -- second component of tuple t
t.2     -- third component (for triples, etc.)
```

Applied to a set of tuples via projection:

```evident
S.0     -- { t.0 | t ∈ S }   -- first components of all tuples in S
S.1     -- { t.1 | t ∈ S }   -- second components of all tuples in S
```

```evident
claim all_sources
    edges   ∈ Set (Node × Node)
    sources ∈ Set Node
    sources = edges.0       -- every node that appears as an edge source

claim all_targets
    edges   ∈ Set (Node × Node)
    targets ∈ Set Node
    targets = edges.1       -- every node that appears as an edge target
```

### Tuple patterns in quantifiers

Tuple patterns bind components directly in `∀` and `∃`:

```evident
∀ (a, b) ∈ S : condition    -- bind both components for each pair in S
∃ (x, y) ∈ S : P(x, y)     -- find a pair in S satisfying P
```

```evident
claim all_edges_valid
    edges ∈ Set (Node × Node)
    nodes ∈ Set Node
    ∀ (u, v) ∈ edges : u ∈ nodes ∧ v ∈ nodes

claim some_connection
    graph ∈ Graph
    src   ∈ Node
    dst   ∈ Node
    ∃ (u, v) ∈ graph.edges : u = src ∧ v = dst
```

---

## Derived operation: `consecutive_pairs`

`consecutive_pairs` is not a built-in — it is a derived field available on `Indexable T`
types. It is defined by the tight binding:

```evident
consecutive_pairs = { (v1, v2) | (i, v1) ∈ entries, (i+1, v2) ∈ entries }
```

For any indexed structure `arr`, `arr.consecutive_pairs` is the set of all pairs of
adjacent values. This is the standard way to express ordering constraints on sequences
without writing explicit index arithmetic at every use site.

```evident
-- Definition (inside Indexable T):
type Indexable T = {
    n                 ∈ Nat
    entries           ⊆ Nat × T
    consecutive_pairs ⊆ T × T
    -- tight binding: determined by entries
    consecutive_pairs = { (v1, v2) | (i, v1) ∈ entries, (i+1, v2) ∈ entries }
    ...
}

-- Use (in claims over Indexable T):
claim non_decreasing[T ∈ Ordered]
    arr ∈ Indexable T
    ∀ (a, b) ∈ arr.consecutive_pairs : a ≤ b

claim no_large_jump
    arr      ∈ Indexable Nat
    max_step ∈ Nat
    ∀ (a, b) ∈ arr.consecutive_pairs : b - a ≤ max_step
```

The tight binding means `consecutive_pairs` is computed once by substitution; the solver
does not search for it. Claims using it read left to right without index bookkeeping.

---

## Operator precedence (set operations)

From the full precedence table (highest to lowest):

1. `.` field access and projection — tightest
2. `[condition]` filter
3. `|S|` cardinality
4. `*`, `/`
5. `+`, `-`
6. `=`, `≠`, `<`, `>`, `≤`, `≥`, `∈`, `∉`, `⊆`
7. `¬`
8. `∧`
9. `∨`
10. `⇒`
11. `∀`, `∃`
12. `·`, `⋈` (composition) — loosest

`S.field[condition].subfield` parses as `((S.field)[condition]).subfield`.
Use parentheses when combining set algebra operators to make precedence explicit:
`(S ∪ T)[.active = true]` is preferred over `S ∪ T[.active = true]`.

---

## Open questions

- Whether `S × T` in a body position introduces element variables automatically
  or only appears on the right side of `∈` and `⊆`
- Syntax for named grouping keys: `grouped_by (.field1, .field2)` for multi-key partitions
- Whether `{a..b}` accepts non-integer bounds (e.g., real intervals) or is Nat/Int only
- Tuple accessor syntax: `.0`, `.1` vs. `fst`, `snd` for pairs specifically
- Whether `consecutive_pairs` should be a built-in operation on `List` and `Array`
  types rather than a user-defined field on `Indexable`
