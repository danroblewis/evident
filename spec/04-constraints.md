# Evident Specification — Constraints

Every constraint in Evident is a membership condition. All constraints in a claim body
are simultaneous — they define an intersection of sets, not a sequence of operations.
The solver finds elements belonging to all sets at once.

---

## Membership constraints

The foundational primitive. Every other constraint form desugars to membership.

```evident
x ∈ S       -- x is a member of set S
x ∉ S       -- x is not a member of S
S ⊆ T       -- every element of S is also in T (S is a subset of T)
S ⊇ T       -- every element of T is also in S (S is a superset of T)
```

In a claim body, `x ∈ S` both binds `x` as a variable ranging over `S` and asserts
that a witness for `x` must exist in `S`. All membership constraints in the same body
are evaluated simultaneously.

```evident
claim valid_assignment
    person ∈ Person
    task   ∈ Task
    task   ∈ person.allowed_tasks   -- further restricts task
```

`S ⊆ T` in a body asserts that the set `S` (already in scope) is contained in `T`.
It does not introduce new element variables.

```evident
claim team_cleared
    team    ∈ Set Person
    project ∈ Project
    team ⊆ project.cleared_personnel   -- every team member is cleared
```

---

## Arithmetic constraints

Arithmetic constraints operate on numeric types (`Nat`, `Int`, `Real`).

### Equality and inequality

```evident
a = b       -- a and b are equal (also: tight binding when a is unbound)
a ≠ b       -- a and b are not equal
```

When `a` is an unbound variable, `a = expression` is a **tight binding** — the solver
eliminates `a` by substitution before search. This is more efficient than leaving `a`
free and is the preferred form for derived quantities.

```evident
claim circle_area
    r    ∈ Real
    area ∈ Real
    area = π * r * r    -- area is tightly bound; no search needed
```

### Ordering

```evident
a < b       -- a is strictly less than b
a ≤ b       -- a is less than or equal to b
a > b       -- a is strictly greater than b
a ≥ b       -- a is greater than or equal to b
```

These extend to any type in `Ordered`.

```evident
claim within_range[T ∈ Ordered]
    value ∈ T
    lower ∈ T
    upper ∈ T
    lower ≤ value
    value ≤ upper
```

### Arithmetic expressions

```evident
a + b       -- sum
a - b       -- difference
a * b       -- product
a / b       -- quotient (real division; integer division is a separate claim)
```

Expressions appear on the right side of `=` or inside ordering constraints.
They are not claims — they are terms that evaluate to values.

```evident
claim budget_check
    items  ∈ Set Purchase
    budget ∈ Nat
    _total = Σ { i.cost | i ∈ items }   -- tight binding via Σ (sum)
    _total ≤ budget
```

### Cardinality

```evident
|S|         -- the number of elements in set S (a Nat)
```

`|S|` is a term, not a claim. It may appear in arithmetic expressions and comparisons.

```evident
claim non_empty[T]
    s ∈ Set T
    |s| ≥ 1

claim exactly_three[T]
    s ∈ Set T
    |s| = 3
```

---

## Logical connectives

### Conjunction

The body of a claim is an implicit conjunction. Every line must hold simultaneously.

```evident
claim valid_booking
    room    ∈ Room
    timeslot ∈ Slot
    room ∈ available_rooms       -- implicit AND
    timeslot ∈ available_slots   -- implicit AND
```

Explicit `∧` is available for inline conjunction within a single expression:

```evident
a ≥ 0 ∧ a < 100     -- both must hold
```

### Disjunction

```evident
P ∨ Q       -- at least one of P or Q holds
```

Disjunction introduces search: the solver must find a branch that satisfies the claim.
Use it when either of two distinct conditions is acceptable.

```evident
claim reachable
    node  ∈ Node
    start ∈ Node
    node = start ∨ ∃ prev ∈ Node : reachable prev start ∧ edge prev node
```

Multiple `evident` clauses for the same claim name are the idiomatic form of disjunction
at the top level — prefer them over `∨` when the cases are structurally distinct.

### Negation

```evident
¬P          -- P does not hold
```

Negation constrains the solver to exclude witnesses that satisfy `P`. It is
negation-as-failure: `¬P` holds when `P` cannot be established given the current evidence.

```evident
claim available
    person  ∈ Person
    timeslot ∈ Slot
    ¬ busy person timeslot
```

### Implication in a body

```evident
P ⇒ Q       -- if P holds then Q must also hold
```

Inside a body, `⇒` is a conditional constraint. If `P` is established, `Q` is required.
If `P` is not established, the constraint is vacuously satisfied.

```evident
claim conditional_discount
    order ∈ Order
    order.total > 100 ⇒ order.discount ≥ 10
```

This differs from a top-level forward rule (see below): inside a body, `⇒` is local to
the claim being defined.

---

## Universal quantification

Universal quantification asserts that a constraint holds for every element of a set.
It does not introduce a witness — it imposes a condition on all members.

### Basic forms

```evident
∀ x ∈ S : P(x)             -- P holds for every x in S
∀ x, y ∈ S : P(x, y)       -- P holds for every pair (x, y) drawn from S
∀ x ∈ S, y ∈ T : P(x, y)  -- P holds for every (x, y) across two sets
```

The multi-variable form `∀ x, y ∈ S` iterates over all ordered pairs where both
elements come from `S` (equivalent to `∀ (x, y) ∈ S × S`).

```evident
claim all_non_negative
    values ∈ Set Int
    ∀ v ∈ values : v ≥ 0

claim no_conflicts
    sessions ∈ Set Session
    ∀ a, b ∈ sessions : a ≠ b ⇒ a.timeslot ≠ b.timeslot
```

### Guarded universal

A guard filters which elements the quantifier applies to.

```evident
∀ x ∈ S : x ≠ special ⇒ P(x)   -- P holds for every x in S except special
```

The guard is expressed as an implication on the right side of `:`. The solver skips
elements where the antecedent fails, making the constraint vacuously satisfied.

```evident
claim non_admin_restricted
    users ∈ Set User
    ∀ u ∈ users : ¬ u.is_admin ⇒ u.access_level ≤ 3
```

### Sugar forms

```evident
∀ S : claim                  -- apply claim to every element of S (names-match)
∀ S[condition] : claim       -- filter S, then apply claim to each matching element
```

The sugar `∀ S : claim` is shorthand when the quantified variable matches the claim's
parameter name. The element is passed by names-match.

```evident
claim all_tasks_scheduled
    tasks ∈ Set Task
    ∀ tasks : is_scheduled       -- each task ∈ tasks is passed to is_scheduled

claim senior_tasks_reviewed
    tasks ∈ Set Task
    ∀ tasks[.priority = High] : is_reviewed   -- only high-priority tasks
```

---

## Existential quantification

Existential quantification asserts that at least one witness exists satisfying a condition.
The solver must find such a witness.

### Explicit form

```evident
∃ x ∈ S : P(x)          -- some x in S satisfies P
∃ x, y ∈ S : P(x, y)    -- some pair (x, y) from S satisfies P
```

Explicit `∃` is used in queries and when you need to name the witness for later use.

```evident
-- Query: is there a solution?
? ∃ assignment ∈ Assignment : valid assignment

-- Body: name the witness and use it below
evident process xs
    ∃ sorted_xs ∈ SortedOf xs
    first_element sorted_xs min_val
    last_element  sorted_xs max_val
```

### Implicit existential (body-only names)

In a body, any variable that appears but is not in the claim head is implicitly
existential. No `∃` declaration is needed.

```evident
claim has_manager
    employee ∈ Person
    manager  ∈ Person           -- implicit: ∃ manager ∈ Person
    reports_to employee manager -- manager is available here
```

Variables beginning with `_` are implicitly existential by convention and signal
that the variable is implementation scaffolding with no domain meaning.

```evident
claim sum_equals
    items ∈ Set Nat
    total ∈ Nat
    _intermediate = Σ items     -- _intermediate: body-internal, solver finds it
    total = _intermediate
```

---

## Cardinality constraints

Cardinality constraints restrict the number of elements satisfying a condition.
They are claims over set comprehensions.

```evident
exactly  n { x ∈ S | P(x) }   -- exactly n elements of S satisfy P
at_most  n { x ∈ S | P(x) }   -- no more than n elements satisfy P
at_least n { x ∈ S | P(x) }   -- at least n elements satisfy P
```

The argument is a set comprehension — callers construct the filtered set at the call site.
This avoids higher-order predicate parameters.

```evident
claim valid_sudoku_row
    cells ∈ List Nat
    |cells| = 9
    ∀ v ∈ {1..9} : exactly 1 { c ∈ cells | c = v }

claim at_most_two_seniors
    team ∈ Set Person
    at_most 2 { p ∈ team | p.level = "senior" }

claim quorum
    voters   ∈ Set Person
    majority ∈ Nat
    majority = |voters| / 2 + 1
    at_least majority { v ∈ voters | v.voted = true }
```

### `all_different`

Asserts that every element of a collection is distinct. Equivalent to
`|{ x | x ∈ S }| = |S|` (deduplication shrinks the set only if elements repeat).

```evident
all_different S     -- all elements of S are distinct
```

```evident
claim no_repeat_assignments
    schedule ∈ Set Assignment
    all_different { a.person | a ∈ schedule }
```

### `unique`

Asserts that exactly one element satisfies a predicate.

```evident
unique x ∈ S : P(x)    -- exactly one x in S satisfies P
```

```evident
claim single_leader
    team ∈ Set Person
    unique p ∈ team : p.role = "lead"
```

---

## Quantifier vocabulary summary

| Symbol | ASCII | Meaning |
|---|---|---|
| `∀` | `all` | every element satisfies |
| `∃` | `some` | at least one element satisfies |
| `∃!` | `one` | exactly one element satisfies |
| `¬∃` | `none` | no element satisfies |

`¬∃ x ∈ S : P(x)` is equivalent to `∀ x ∈ S : ¬P(x)`. Both forms are accepted;
`none` reads more clearly when the intent is absence.

```evident
claim no_conflicts
    events ∈ Set Event
    none e ∈ events : overlaps e.timeslot e.room

claim unique_winner
    contestants ∈ Set Person
    one p ∈ contestants : p.score = max_score
```

---

## Tight bindings

A tight binding uses `=` to pin an unbound variable to a deterministic expression.
The solver eliminates the variable by substitution — no search is needed for it.

```evident
x = expression      -- x is constrained to equal expression (deterministic)
```

Tight bindings are most valuable for derived quantities: values that follow uniquely from
other variables in scope. Naming them makes subsequent constraints readable.

```evident
claim schedule_fits
    tasks  ∈ Set Task
    window ∈ Nat
    _total_duration = Σ { t.duration | t ∈ tasks }   -- tight: determined by tasks
    _total_duration ≤ window

claim indexed_access
    arr   ∈ Indexable Nat
    _last_idx = arr.n - 1                             -- tight: determined by arr.n
    ∀ i ∈ {0.._last_idx} : arr.entries[.0 = i] ≠ {}
```

For `det` claims, the result is bound by `=` on the left:

```evident
_n = length list    -- _n uniquely determined: length is det
_k = |some_set|     -- _k is the cardinality, uniquely determined
```

Tight bindings compose: if `a = f(x)` and `b = g(a)`, both are eliminated before search.
The solver sees only the irreducibly free variables.

---

## Top-level forward rules

At the top level (outside a claim body), `⇒` is a **forward rule**: if the left side
is established in the evidence base, the right side becomes immediately evident.
Forward rules fire at fixpoint during constraint propagation, before search.

```evident
card_valid ⇒ payment_authorized

admin_user u ⇒ can_access u any_resource

∃ s ∈ SortedOf xs, first_element s m ⇒ minimum_of xs m
```

Forward rules extend the evidence base monotonically. They cannot retract facts.

---

## Open questions

- Syntax for `none` / `¬∃` as a quantifier vs. as a sugar over `∀ ... : ¬`
- Whether `unique` is sugar for `exactly 1 { x ∈ S | P(x) }` or a distinct form
- Interaction between `∨` and multiple `evident` clauses — which is preferred when?
- Whether `P ⇒ Q` inside a body desugars to `¬P ∨ Q` for the solver
