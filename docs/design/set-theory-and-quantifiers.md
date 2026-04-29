# Set Theory as Evident's Foundation

## The `?` was wrong

In `assignment_valid`, we wrote:

```evident
find_worker a.worker_id workers ?worker
```

The `?` was borrowed from Prolog and Datalog, where variables are marked to distinguish
them from constants. But it implies assignment, and there is no assignment. It was the
wrong tool.

What we were trying to express is an **existential claim**: there exists a worker in the
collection whose id matches. The solver doesn't need us to mark variables specially — it
already knows that `worker` is a free name that must be resolved to satisfy the constraints.
The `?` was noise.

---

## What `assignment_valid` is actually trying to say

We have three things:

- `workers` — a list of `Worker` records
- `tasks` — a list of `Task` records
- `a` — an `Assignment` record with `worker_id : Nat`, `task_id : Nat`, `start : Nat`

The types are connected by id, not by nesting. To check if the assignment is valid,
we need to reach through the id to get the actual worker and task records, then check
time arithmetic. That "reaching through" is a **join** — the fundamental operation
of relational algebra.

In plain English: *there exists a worker in the list whose id matches the assignment,
and there exists a task in the list whose id matches, and the time constraints hold
for that worker and task.*

In set theory:

```
∃ w ∈ workers, ∃ t ∈ tasks :
    w.id = a.worker_id  ∧
    t.id = a.task_id    ∧
    a.start ≥ w.available_from  ∧
    a.start + t.duration ≤ w.available_until
```

---

## Is set theory naturally part of Evident?

Yes, completely.

Evident's underlying execution model — facts in a monotonically growing base, rules that
derive new facts, a fixpoint — is exactly Datalog semantics, which is exactly the
relational algebra, which is exactly first-order logic over sets. The whole thing IS set
theory. We just haven't surfaced that in the syntax.

The quantifiers of set theory map directly to what Evident needs:

| Set theory | What it means | Status in Evident |
|---|---|---|
| `∃ x ∈ S : P(x)` | some element of S satisfies P | currently: `?x in S, P(x)` — awkward |
| `∀ x ∈ S : P(x)` | every element of S satisfies P | currently: hand-written recursion |
| `{ x ∈ S \| P(x) }` | the subset of S satisfying P | currently: not expressible inline |
| `x ∈ S` | x is a member of S | `x in S` — works |

---

## Rewriting with set-theoretic quantifiers

If `some` and `all` are first-class keywords, `assignment_valid` becomes:

```evident
evident assignment_fits workers tasks a
    some w in workers : w.id = a.worker_id
    some t in tasks   : t.id = a.task_id
    a.start >= w.available_from
    a.start + t.duration <= w.available_until
```

`some w in workers : w.id = a.worker_id` means: there exists a worker in the collection
with this id; call it `w` in subsequent lines. No `?`, no assignment, just quantification.
The name `w` is introduced by the quantifier, not by assignment — it is available
in the lines below. If no such worker exists, the whole claim fails.

The universal version is equally natural:

```evident
evident tasks_covered_by tasks schedule
    all t in tasks : some a in schedule : a.task_id = t.id
```

Read: "for every task t in tasks, there exists an assignment a in schedule such that
a references t." No helper claims. No hand-written recursion. The quantifiers express
the structure directly.

### Before and after

**Before:**
```evident
evident assignment_valid workers tasks a
    find_worker a.worker_id workers ?worker
    find_task   a.task_id   tasks   ?task
    a.start >= worker.available_from
    a.start + task.duration <= worker.available_until
```

**After:**
```evident
evident assignment_fits workers tasks a
    some w in workers : w.id = a.worker_id
    some t in tasks   : t.id = a.task_id
    a.start >= w.available_from
    a.start + t.duration <= w.available_until
```

The `find_worker` and `find_task` helper claims disappear entirely.
The `?` disappears. The join is expressed directly as what it is: an existential claim
over a set, with the witness named for use below.

---

## The nested types alternative

The reason we needed the join at all is that `Assignment` stores `worker_id : Nat`
instead of `worker : Worker`. This is the database style — flat types connected by id.

If we nested instead:

```evident
type Assignment = {
    worker : Worker
    task   : Task
    start  : Nat
}
```

Then `assignment_fits` collapses to two lines with no quantification needed:

```evident
evident assignment_fits a
    a.start >= a.worker.available_from
    a.start + a.task.duration <= a.worker.available_until
```

No join, no existential, no helper claims. The types carry the relationship structurally.

**The tradeoff:**

| | Flat / id-based | Nested |
|---|---|---|
| Style | Database schema | Object graph |
| Joins | Required, explicit | Not needed |
| Query flexibility | High — ask "all tasks for Alice" by querying assignments | Lower — schedule is opaque |
| Constraint simplicity | More setup | Cleaner once set up |
| Duplication | Worker stored once, referenced by id | Worker data repeated in each assignment |

Both are valid. Flat schemas are better when you need to ask cross-cutting questions
about the collection. Nested schemas are better when each record is self-contained.

---

## What this means for the language

If set theory is Evident's foundation, then `some`, `all`, and set comprehension should
be keywords, not library functions. The solver already reasons in set-theoretic terms —
it finds witnesses for existential claims and checks universals. Making this explicit in
the syntax means the programmer writes in the same language the solver thinks in.

This also dissolves the `find_X` naming problem from the grammar rules entirely.
You never need a named claim for a lookup. Named claims are for domain logic that
recurs in multiple places, not for one-off joins over a collection.

### The scoping question

`some w in workers : w.id = a.worker_id` introduces `w` as a name available in
*subsequent* body lines. This is necessary — we reference `w.available_from` later.

This is different from assignment. `w` is not a stored value. It is the witness to the
existential claim. If the solver finds multiple workers matching the condition, the claim
holds for any of them (and the solver will pick one or explore all, depending on whether
the claim is `semidet` or `nondet`). If none match, the whole claim fails.

The open design question: how far down does `w` scope? Just the current `evident` block?
For now: a name introduced by `some` is available for the remainder of the body block
it appears in.

---

## The quantifier vocabulary

Proposed first-class keywords:

```evident
-- Existential: there exists at least one
some w in workers : w.id = a.worker_id

-- Universal: every element satisfies
all t in tasks : deadline_met t schedule

-- Unique: exactly one exists
one a in schedule : a.task_id = t.id

-- None: no element satisfies
none a in schedule : a.worker_id = w.id, overlap a other

-- Comprehension: the subset satisfying a condition
{ a in schedule | a.worker_id = w.id }   -- all of alice's assignments

-- Count
count { a in schedule | a.worker_id = w.id } >= 1
```

These are all expressible as claims in the current system (via recursion), but making
them keywords means:

1. The solver can use efficient set algorithms instead of recursive proof search
2. The programmer writes intent directly, not a recursive encoding of intent
3. The syntax signals to the reader "this is a quantifier over a collection"

`all t in tasks : P(t)` compiled to a recursive claim is an O(n) proof search.
The same expression compiled to a set-theoretic check can be O(n) but with much lower
constant factors, and can be parallelized trivially.
