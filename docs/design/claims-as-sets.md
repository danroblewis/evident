# Claims as Sets: The Unified Model

The key insight: **type definitions and evident blocks are both set-builder expressions**.
They look slightly different on the surface but they are doing the same thing —
defining a set by specifying membership conditions.

---

## Type definitions are set-builder notation

```evident
type Task = {
    id       ∈ Nat
    duration ∈ Nat
    deadline ∈ Nat
}
```

This defines `Task` as the set of all records satisfying these membership conditions.
In full set-builder notation:

```
Task = { t | t.id ∈ Nat ∧ t.duration ∈ Nat ∧ t.deadline ∈ Nat }
```

`Task` is not a template or a schema — it **is** a set. Any value belonging to all
three field-sets simultaneously is a member of `Task`. The type definition is
exactly a set intersection:

```
Task = Nat_id ∩ Nat_duration ∩ Nat_deadline
```

where `Nat_id` is the set of records whose `id` field is a natural number, etc.

---

## Claim declarations name a set

```evident
claim valid_schedule : List Task → List Worker → Schedule → Prop
```

This declares that `valid_schedule` names a set — specifically, a set of triples
`(tasks, workers, schedule)`. The declaration says nothing about which triples are
in the set. It just assigns the name.

`Prop` at the end is what distinguishes this from a function. It means the claim
produces a truth value — equivalently, it defines a **subset** of its input domain.
`valid_schedule` names the subset of `(List Task × List Worker × Schedule)` triples
that are valid.

---

## Evident blocks add membership conditions

Each `evident` block adds a condition that must be satisfied for something to belong
to the set:

```evident
-- Step 1: valid_schedule ⊆ { triples where every task is covered }
evident valid_schedule tasks workers schedule
    ∀ t ∈ tasks : ∃ a ∈ schedule : a.task_id = t.id
```

```evident
-- Step 2: valid_schedule ⊆ Step1 ∩ { triples where all assignments are feasible }
evident valid_schedule tasks workers schedule
    ∀ t ∈ tasks : ∃ a ∈ schedule : a.task_id = t.id
    ∀ a ∈ schedule : assignment_fits workers tasks a
```

```evident
-- Step 3: valid_schedule ⊆ Step2 ∩ { triples where no worker is double-booked }
evident valid_schedule tasks workers schedule
    ∀ t ∈ tasks : ∃ a ∈ schedule : a.task_id = t.id
    ∀ a ∈ schedule : assignment_fits workers tasks a
    ∀ a ∈ schedule, ∀ b ∈ schedule : a ≠ b, a.worker_id = b.worker_id ⇒ no_overlap a b tasks
```

```evident
-- Step 4: valid_schedule = Step3 ∩ { triples where all deadlines are met }
evident valid_schedule tasks workers schedule
    ∀ t ∈ tasks : ∃ a ∈ schedule : a.task_id = t.id
    ∀ a ∈ schedule : assignment_fits workers tasks a
    ∀ a ∈ schedule, ∀ b ∈ schedule : a ≠ b, a.worker_id = b.worker_id ⇒ no_overlap a b tasks
    ∀ t ∈ tasks, ∀ a ∈ schedule : a.task_id = t.id ⇒ a.start + t.duration ≤ t.deadline
```

Each step is intersecting the previous set with a new constraint set.
The body of an `evident` block is exactly a set-builder condition.

---

## Constraint accumulation is progressive set intersection

```
S₀ = Schedule                           -- all possible schedules (no constraint)
S₁ = S₀ ∩ { s | tasks all covered }    -- after adding constraint ①
S₂ = S₁ ∩ { s | assignments feasible } -- after adding constraint ②
S₃ = S₂ ∩ { s | no overlaps }          -- after adding constraint ③
S₄ = S₃ ∩ { s | deadlines met }        -- after adding constraint ④

valid_schedule tasks workers = S₄
```

The set shrinks at each step. The programmer's job is to add enough constraints
that the final set contains exactly the elements they want — and ideally only one,
if the solution is unique.

When the solver answers `? valid_schedule tasks workers ?s`, it finds an element
in `S₄`. If `S₄` is empty, no valid schedule exists. If `S₄` has one element,
there is a unique valid schedule. If `S₄` has many, the solver returns one
(or all, if asked).

---

## Every claim names a set

This generalizes:

| Claim | Set it names |
|---|---|
| `claim sorted : List Nat → Prop` | the set of sorted lists |
| `claim member : Nat → List Nat → Prop` | the set of (n, list) pairs where n appears in list |
| `claim factorial : Nat → Nat → det` | the set of (n, f) pairs where f = n! |
| `claim assignment_fits : List Worker → List Task → Assignment → Prop` | the set of feasible (workers, tasks, assignment) triples |

The `evident` blocks define which elements belong to each set, by stating membership
conditions in terms of other sets (other claims). The whole program is a network of
sets defined by their relationships to each other.

---

## The unified syntax

Type definitions and claim definitions are now visibly the same thing:

```evident
-- A type: defines a set by field membership
type Task = {
    id       ∈ Nat
    duration ∈ Nat
    deadline ∈ Nat
}

-- A claim: defines a set by relational membership conditions
-- (the evident blocks provide the set-builder body)
claim valid_schedule : List Task → List Worker → Schedule → Prop

evident valid_schedule tasks workers schedule
    ∀ t ∈ tasks : ∃ a ∈ schedule : a.task_id = t.id
    ∀ a ∈ schedule : assignment_fits workers tasks a
    ...
```

Both say: "a member of this set must satisfy these conditions."
The difference is only in what the conditions look like:

- **Type definitions**: structural field conditions (`id ∈ Nat`)
- **Claim definitions**: relational conditions (`∀ t ∈ tasks : ...`)

These could be written in an even more unified form. A type is just a claim where
the membership conditions are field-type constraints:

```evident
-- These are equivalent ways to define the same set:

type Task = { id ∈ Nat, duration ∈ Nat, deadline ∈ Nat }

claim Task : Record → Prop
evident Task r
    r.id       ∈ Nat
    r.duration ∈ Nat
    r.deadline ∈ Nat
```

`type` is syntax sugar for a claim whose membership conditions are field memberships.

---

## What the solver is doing

The solver is a **set intersection engine**. Given a query, it:

1. Identifies which set the query is asking about (e.g. `valid_schedule tasks workers`)
2. Enumerates the membership conditions that define that set (from the `evident` blocks)
3. Checks each condition, propagating constraints to narrow the search space
4. Returns an element of the set — a witness — along with evidence that it satisfies
   all membership conditions

The evidence term for a claim is exactly the **certificate of set membership**: a proof
that the returned value satisfies all the membership conditions, tracing back through
all the sub-claims to the axioms.

```
valid_schedule(tasks, workers, s) is evident
    ↳ because s ∈ { schedules where all tasks covered }       ← sub-evidence ①
    ↳ because s ∈ { schedules where assignments feasible }    ← sub-evidence ②
    ↳ because s ∈ { schedules where no overlaps }             ← sub-evidence ③
    ↳ because s ∈ { schedules where deadlines met }           ← sub-evidence ④
```

Each sub-evidence is itself a set membership certificate. The full evidence tree is
the proof that `s` is in the intersection of all four constraint sets.

---

## Why this is the right mental model

This framing explains several things that were previously described separately:

**Why order doesn't matter**: set membership conditions are conjunctions — `A ∩ B = B ∩ A`.
The order you check membership conditions is irrelevant; what matters is that all are satisfied.

**Why underconstrained programs return garbage**: if you define only one membership condition
instead of four, the set is very large and contains many elements the programmer didn't want.
Adding constraints shrinks the set toward the target.

**Why `find_X` claims were wrong**: you don't "find" elements of a set procedurally.
You state the membership condition and the solver locates elements satisfying it.
`∃ w ∈ workers : w.id = a.worker_id` is the membership condition; the solver finds `w`.

**Why named claims are useful**: they give reusable names to sets that appear in multiple
membership conditions. `assignment_fits` is not a procedure — it's a named set of feasible
assignments. Naming it allows other conditions to reference it concisely.

**Why the language is order-independent**: set intersection is commutative and associative.
The program defines a set; the solver finds its members. Neither operation has an inherent order.
