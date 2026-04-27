# Evident Specification — Naming Conventions

Naming rules in Evident are not enforced by the parser. They are design principles
that keep programs readable, composable, and genuinely relational. Violating them
does not produce a parse error; it produces programs that feel procedural, resist
reuse, and communicate poorly.

The central question for any name: **can you read it as a noun phrase describing a
fact about the world?** If the answer is no, the name is wrong.

---

## Claim names: noun phrases only

A claim names a set. Its name should describe membership in that set — what it
means to be an element — not the process of finding or computing elements.

**Good names — they describe relationships and states:**

| Name | Reads as |
|---|---|
| `sorted` | "this list is sorted" |
| `valid_schedule` | "this schedule is valid" |
| `in_order` | "these elements are in order" |
| `deadline_met` | "this deadline is met" |
| `adjacent` | "these nodes are adjacent" |
| `assignment_feasible` | "this assignment is feasible" |
| `within_budget` | "this plan is within budget" |
| `tasks_covered_by` | "these tasks are covered by this schedule" |

**Bad names — they describe operations, not facts:**

| Name | Why it is wrong |
|---|---|
| `find_worker` | describes a search step |
| `compute_gcd` | describes a calculation |
| `validate_request` | describes a procedure |
| `sort_list` | describes an action |
| `get_task` | describes retrieval |
| `fetch_token` | describes I/O |
| `build_schedule` | describes construction |
| `check_deadline` | describes a check |

The test: swap the claim name into the sentence "It is the case that ___."
`sorted` — "It is the case that [this list is] sorted." — reads as a fact.
`sort_list` — "It is the case that sort_list" — does not describe a state.

---

## Forbidden name prefixes

These prefixes signal that a name describes an operation rather than a relationship.
They are never acceptable as claim names:

```
find_     get_      compute_   fetch_
calculate_ make_    check_     validate_
build_    process_  handle_    run_
```

If you reach for one of these, replace the entire claim with an inline existential
(see Rule 4 in grammar-rules.md) or rename to a state-describing phrase.

---

## Type names: PascalCase

Type names use `PascalCase`. Each word begins with an uppercase letter, no
underscores. Type names denote sets of values — they are nouns naming a category.

```evident
Task
Worker
Assignment
ValidRequest
ConferenceSchedule
TimedTask
BoundedNat
```

Type names beginning with a lowercase letter are parse errors. Names that mix
cases unpredictably (`task_Type`, `WORKER`) are grammatical but wrong.

---

## Variable names: lowercase_snake_case

Variables (claim parameters and body-only names) use `lowercase_snake_case`.
All letters lowercase; words separated by underscores.

```evident
schedule
max_parallel
slot_assignments
available_from
consecutive_pairs
```

Do not use camelCase for variables (`maxParallel`, `slotAssignments`). Do not
use single-letter names except for universally-quantified dummy variables in
`∀` and `∃` expressions where the name genuinely carries no meaning.

---

## Body-internal scaffolding: underscore prefix

Any variable introduced inside a claim body whose value has no domain meaning —
it exists only to decompose an expression or name an intermediate relationship —
takes a `_` prefix.

```evident
claim product_of
    a, b, c ∈ Nat

evident product_of (succ a) b c
    _partial = product_of a b   -- solver finds _partial; it is not part of the interface
    c        = _partial + b

claim occurrences_of[T ∈ Eq]
    x    ∈ T
    list ∈ List T
    n    ∈ Nat

evident occurrences_of x [] 0

evident occurrences_of x (cons h rest) n
    _n0 = occurrences_of x rest   -- internal: count in tail
    n   = _n0 + 1  when  h = x
    n   = _n0      when  h ≠ x
```

`_partial` and `_n0` have no domain meaning. They are scaffolding. The solver
finds their values. They are not accessible from outside the claim.

Names without the `_` prefix that appear only in the body are also implicitly
existential — the solver finds values for them too. The underscore is a readability
signal to the reader: "this has no semantic content worth naming."

Use the `_` prefix when:
- The variable is a pure decomposition artifact
- It would be confusing or misleading to give it a domain name
- It should never appear in an external constraint

Do not use the `_` prefix when the body-only variable genuinely refers to a domain
entity that happens not to be in the head — give it a descriptive name:

```evident
evident assignment_fits_window workers tasks a
    worker ∈ workers, worker.id = a.worker_id   -- not _worker: this IS a domain worker
    task   ∈ tasks,   task.id   = a.task_id     -- not _task: this IS a domain task
    a.start ≥ worker.available_from
    a.start + task.duration ≤ worker.available_until
```

---

## Multi-argument claims: name the whole relationship

A claim with N arguments describes a relationship among all N. The name must
reflect the entire relationship, not just the "main" argument.

```evident
-- Wrong: name only reflects one argument
claim worker_check workers tasks assignment

-- Right: name reflects the relationship
claim assignment_fits workers tasks assignment

-- Also right: if feasibility is the core concept
claim feasible assignment workers tasks
```

For symmetric relationships, either order of arguments is acceptable. For
asymmetric ones, put the primary subject first (the thing being described) and
the context second (the thing it is being described relative to).

```evident
claim member_of[T ∈ Eq]
    x    ∈ T
    list ∈ List T
    ∃ (_, v) ∈ list : v = x
-- "x is a member of list" — x is subject, list is context
```

---

## Use `∈` directly — no `member` claim

Do not define or use a `member` claim. Write `x ∈ collection` directly. The `∈`
operator is a built-in primitive; wrapping it in a claim adds no information and
increases noise.

```evident
-- Wrong
member x list

-- Right
x ∈ list
```

For filtered membership, write the constraint inline:

```evident
-- Wrong
member_satisfying x list condition

-- Right
x ∈ list, condition x
```

---

## No lookup claims — use inline existentials

Do not define claims whose sole purpose is to navigate a collection and return
a matching element. Replace them with inline membership constraints. The solver
performs the search; no dedicated claim is needed.

```evident
-- Wrong: lookup claim
evident assignment_fits workers tasks a
    find_worker a.worker_id workers ?worker
    find_task   a.task_id   tasks   ?task
    a.start ≥ worker.available_from
    a.start + task.duration ≤ worker.available_until

-- Right: inline existentials
evident assignment_fits workers tasks a
    worker ∈ workers, worker.id = a.worker_id
    task   ∈ tasks,   task.id   = a.task_id
    a.start ≥ worker.available_from
    a.start + task.duration ≤ worker.available_until
```

`worker ∈ workers, worker.id = a.worker_id` is a membership constraint. There
must exist an element of `workers` with matching id. The solver finds it. No
separate claim encodes this.

---

## Prefer universal statements over case analysis

Before writing multiple `evident` blocks for the same claim, ask whether a single
universal statement covers all cases. Base cases are usually vacuously true: a `∀`
over an empty collection holds automatically.

```evident
-- Wrong: three evident blocks for three cases
evident sorted []
evident sorted [_]
evident sorted (cons a (cons b rest))
    a ≤ b
    sorted (cons b rest)

-- Right: one statement; empty and singleton cases are vacuous
claim sorted[T ∈ Ordered]
    list ∈ List T
    ∀ (a, b) ∈ list.consecutive_pairs : a ≤ b
```

Multiple `evident` blocks are appropriate only when the claim's arguments have
genuinely distinct structural forms — different constructors of an algebraic type —
that cannot be unified into a single universal statement.

---

## Data structures as constrained graphs

Do not treat lists, trees, and sequences as axiomatically given primitives in your
domain models. Define them as sets of index-value pairs (or node-edge pairs for
graphs) with constraints that enforce the required structure. The `Indexable`
interface in the standard library (see spec 09) shows this pattern.

Higher-level domain types should be built on these structural definitions:

```evident
type Schedule = {
    assignments ⊆ Nat × Assignment     -- indexed collection of assignments
    ∀ (i, _), (j, _) ∈ assignments : i ≠ j  -- distinct indices (consequence of ⊆ Nat × ...)
}
```

This keeps the semantics explicit and the solver's job uniform.

---

## The tight binding pattern

When a complex expression is used in more than one place, name it with a defining
equation. The solver eliminates tight bindings by substitution before search — they
carry no runtime cost.

```evident
claim schedule_conflict_free
    schedule ∈ Schedule

    consecutive_pairs ⊆ Assignment × Assignment
    consecutive_pairs = { (a1, a2) | (i, a1) ∈ schedule.assignments,
                                     (i+1, a2) ∈ schedule.assignments }

    ∀ (a1, a2) ∈ consecutive_pairs : a1.end ≤ a2.start
```

`consecutive_pairs` is a tight binding. It is not a claim; it is a named
expression defined by `=` within the body. The solver replaces it by its
definition everywhere it appears.

Do not confuse tight bindings with claim invocations. A tight binding is `name =
expression`; it introduces a local name for a set-comprehension or arithmetic
expression. A claim invocation is `claim_name args`; it joins two constraint
systems by identifying their shared variables.

---

## Summary

| Category | Convention | Example |
|---|---|---|
| Claim names | noun phrases, `lowercase_snake_case` | `deadline_met`, `valid_schedule` |
| Type names | `PascalCase` | `Task`, `ValidAssignment` |
| Variable names | `lowercase_snake_case` | `schedule`, `max_parallel` |
| Internal scaffolding | `_` prefix | `_partial`, `_n0` |
| Forbidden claim prefixes | action verbs | `find_`, `get_`, `compute_`, `validate_` |
| Collection membership | inline `∈` | `x ∈ list`, not `member x list` |
| Data navigation | inline existential | `w ∈ workers, w.id = id` |
| Case analysis | universal `∀` when possible | `∀ (a, b) ∈ list.consecutive_pairs` |

---

## Open questions

- **Claim names for optimization criteria**: claims like `optimal_schedule` or
  `minimum_cost_assignment` describe a state but may be confusing because they
  imply a unique witness. Whether these should be named differently from
  satisfiability claims is not yet settled.

- **Multi-word type names**: `ValidAssignment` vs `Valid_Assignment` — currently
  PascalCase is required (no underscores in type names), but this has not been
  formally decided.

- **Claim name directionality**: `assignment_fits_worker` vs `worker_fits_assignment`
  — the convention says "primary subject first," but what counts as primary is
  sometimes ambiguous. No mechanical rule resolves this; it requires judgment.
