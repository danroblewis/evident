# Evident Grammar Rules

Rules for keeping Evident programs relational rather than procedural.
These are not enforced by the parser — they are design principles that
prevent the language from collapsing back into functional programming.

---

## Rule 1: Claim names describe states and relationships, never actions

A claim name answers the question "what is true?" not "what should be done?"

**Forbidden patterns — action verbs as names:**

| Wrong | Why | Right |
|---|---|---|
| `find_worker id workers` | describes a search operation | `worker_in workers, .id = id` |
| `get_task id tasks` | describes retrieval | `task_in tasks, .id = id` |
| `compute_makespan schedule` | describes a computation | `makespan_of schedule` |
| `fetch_token header` | describes an I/O operation | `token_from header` |
| `check_deadline task assignment` | describes a procedure | `deadline_met task assignment` |
| `validate_request req` | describes a validation step | `request_valid req` |
| `build_schedule tasks workers` | describes construction | `valid_schedule tasks workers` |

**Good name patterns:**

- `X_valid` — X satisfies its requirements
- `X_for Y` — X is the appropriate thing for Y
- `X_in Y` — X is a member of collection Y
- `X_of Y` — X is a property/component of Y
- `X_and_Y_overlap` — the relationship between X and Y
- `X_satisfies Y` — X meets condition Y
- `X_before Y` — temporal relationship
- `X_within Y` — containment relationship

The test: can you read the claim name as a noun phrase describing a fact about the world?
`deadline_met` — yes, that's a state. `compute_deadline` — no, that's an instruction.

---

## Rule 2: Body-only names are implicitly existential — use `_` for scaffolding

Any name that appears in a body but not in the head is implicitly existentially
quantified — the solver finds a value for it. No `∃` declaration and no `?` prefix
required in body blocks.

Use the `_` prefix convention for names that are implementation scaffolding with
no meaningful domain name:

```evident
-- _partial is body-internal: solver finds it, no domain significance
evident product (succ a) b c
    _partial = product a b
    c        = sum _partial b
```

Names without `_` that appear only in the body are also implicitly existential —
the underscore is a readability convention, not a syntax rule.

**Head names** (parameters after `evident`) are bound from the outside.
**Body-only names** are found by the solver.
All body conditions are simultaneous — there is no ordering.

---

## Rule 3: `det` claims use `= claim args` binding form ⚠ UNDER RECONSIDERATION

> **This rule is in question.** The `=` sign implies function evaluation and
> directional assignment, but claim invocation in a body is constraint joining —
> variable identification across two constraint systems. `=` gives the wrong
> mental model. The correct notation is unresolved. See `what-we-learned.md`
> section "Claim composition is variable identification."

~~A `det` claim has exactly one result for any given inputs — it is a function.
Use `=` to bind its result, not a positional output argument:~~

The existing examples use `= claim args` notation but this is likely wrong in principle.
It is kept in the examples for now as a placeholder until the correct syntax is settled.

The distinction that is still valid: `det` (exactly one satisfying value) vs `semidet`
(zero or one) vs `nondet` (zero or more) is a real and useful property of claims.
What is wrong is using `=` to express the interaction — not the distinction itself.

---

## Rule 4: Use inline membership instead of lookup claims

Whenever you find yourself writing a `find_X` or `get_X` claim, replace it with
an inline membership constraint. Claims should encode domain logic, not navigation.

**Before (procedural):**

```evident
evident assignment_fits workers tasks a
    find_worker a.worker_id workers ?worker
    find_task   a.task_id   tasks   ?task
    a.start >= worker.available_from
    a.start + task.duration <= worker.available_until
```

**After (relational):**

```evident
evident assignment_fits workers tasks a
    _w ∈ workers, _w.id = a.worker_id
    _t ∈ tasks,   _t.id = a.task_id
    a.start ≥ _w.available_from
    a.start + _t.duration ≤ _w.available_until
```

`_w ∈ workers, _w.id = a.worker_id` is a membership constraint: there exists an
element of `workers` with matching id. The solver finds it. No claim needed.

---

## Rule 5: Multi-argument claim names describe the relationship between all arguments

A claim with N arguments describes a relationship involving all N of them.
The name should reflect the whole relationship, not just the "main" argument.

**Wrong — name only reflects one argument:**
```evident
claim worker_check : Worker -> Task -> Assignment -> Prop
-- What does this check? Only about the worker? What about task and assignment?
```

**Right — name reflects the relationship:**
```evident
claim assignment_fits_worker : Assignment -> Worker -> Task -> Prop
-- "this assignment fits within this worker's availability for this task"
```

Or even better, if the claim is really about three things interacting:
```evident
claim feasible : Assignment -> Worker -> Task -> Prop
-- "this assignment is feasible given this worker and task"
```

---

## Rule 6: Arguments are ordered by dependency, not by procedural role

In functional programming, argument order often reflects the flow of data:
input first, output last. In Evident, argument order should reflect
the natural reading of the relationship.

For a symmetric relationship, either order should work:
```evident
claim overlap : Assignment -> Assignment -> Prop
-- a and b overlap — symmetric, order doesn't matter semantically
```

For an asymmetric relationship, put the primary subject first:
```evident
claim member_of[T] : T -> List T -> Prop
-- "x is a member of list" — x is the subject, list is the context
```

For containment, put the contained thing first:
```evident
claim in_window : Nat -> Nat -> Nat -> Prop
-- "time t falls within window [from, until]"
-- usage: a.start in_window worker.available_from worker.available_until
```

---

## Rule 7: Body lines are constraints, not instructions

Each line in a body block states something that must be true.
It should read as a constraint on the world, not a step in a procedure.

**Procedural reading (wrong mental model):**
> "First find the worker, then find the task, then check the start time..."

**Constraint reading (correct mental model):**
> "These things must all be simultaneously true: a matching worker exists,
> a matching task exists, the start time is within the worker's window,
> and the task fits before the worker leaves."

The solver establishes them in whatever order makes sense. You don't specify order.

This means body lines should be things that could be true or false — not things
that "do" something. If a line feels like an instruction, rewrite it as a constraint.

**Instruction (wrong):**
```evident
-- "loop through all tasks and assign each one"
assign_all tasks schedule
```

**Constraint (right):**
```evident
-- "every task appears as a task_id in some assignment in the schedule"
?t in tasks => ?a in schedule, ?a.task_id = ?t.id
```

---

## Rule 8: No claim should be needed solely for data retrieval

If the only reason a claim exists is to look something up by id or key,
replace it with an inline existential. Claims should encode domain logic,
not data navigation.

**Data retrieval claims to eliminate:**
- `find_X id collection ?result` → `?result in collection, ?result.id = id`
- `get_field record ?value` → just use `record.field` directly
- `lookup key map ?value` → `?value = map[key]` or `(key, ?value) in map`

**Claims worth keeping — they encode domain logic:**
- `deadline_met task assignment` — encodes the business rule about deadlines
- `assignment_feasible workers tasks a` — encodes multiple business constraints
- `schedule_optimal schedule tasks` — encodes an optimization criterion

---

## Applied: rewriting the scheduling example

**Before (procedural names, lookup claims):**

```evident
evident valid_schedule tasks workers schedule
    all_tasks_assigned tasks schedule
    all_assignments_valid workers tasks schedule
    no_overlapping_assignments schedule
    all_deadlines_met tasks schedule

evident assignment_valid workers tasks a
    find_worker a.worker_id workers ?worker
    find_task   a.task_id   tasks   ?task
    a.start >= worker.available_from
    a.start + task.duration <= worker.available_until
```

**After (relational names, inline existentials):**

```evident
evident valid_schedule tasks workers schedule
    tasks_covered_by schedule
    schedule_uses_only workers tasks
    schedule_overlap_free
    tasks_meet_deadlines tasks schedule

evident schedule_uses_only workers tasks schedule
    ?a in schedule =>
        ?worker in workers, ?worker.id = ?a.worker_id
        ?task   in tasks,   ?task.id   = ?a.task_id
        ?a.start >= ?worker.available_from
        ?a.start + ?task.duration <= ?worker.available_until
```

The names now describe states:
- `tasks_covered_by` — the tasks are covered by the schedule
- `schedule_uses_only` — the schedule uses only valid workers/tasks
- `schedule_overlap_free` — no overlaps in the schedule
- `tasks_meet_deadlines` — all deadlines are satisfied

And the `find_worker` / `find_task` helper claims are gone entirely,
replaced by inline existentials in the body.

---

## Rule 8a: A claim is a named list of constraints

The `claim` keyword introduces a name. Everything indented below is a constraint —
type membership constraints and relational constraints are the same kind of thing.
There is no structural distinction between "the parameter list" and "the body."

```evident
claim distance_between
    g ∈ Graph           -- constraint: g must be in the Graph set
    a ∈ g.nodes         -- constraint: a must be a node in g
    b ∈ g.nodes         -- constraint: b must be a node in g
    d ∈ Nat             -- constraint: d must be a natural number
    path_between g a b _path
    path_length _path d
```

Named variables (no `_` prefix) are the claim's interface — accessible from outside.
Variables with `_` prefix are internal scaffolding.

There is **no `: Type` annotation** at the end of claim signatures. There is no
return type because there is no return. All variables are declared as constraints.

---

## Rule 8b: Merge `claim` and `evident` when there is one definition

When a claim has exactly one body, the `claim` declaration and `evident` block
are redundant. Merge them — the body follows the claim name, indented.
All constraints (type membership and relational) are listed together:

```evident
-- Old (two blocks):
claim acyclic : Prop
evident acyclic
    ∀ n ∈ nodes : ¬ in_cycle n

-- New (one flat constraint list):
claim acyclic
    ∀ n ∈ nodes : ¬ in_cycle n
```

With variables:

```evident
-- Old:
claim shortest_path_between : Nat → Nat → List Nat → semidet
evident shortest_path_between a b path
    ...

-- New:
claim shortest_path_between
    a, b ∈ Nat
    path ∈ List Nat
    ...
```

Type parameters stay in `[...]`:

```evident
claim sorted[T ∈ Ordered]
    list ∈ List T
    ∀ (a, b) ∈ each_consecutive list : a ≤ b
```

Use separate `claim` + `evident` blocks only when genuinely needed:
multiple alternative definitions (structurally distinct cases).

---

## Rule 9: Claim composition — named mapping and pass-through

When a sub-claim is invoked in a body, its variables are identified with variables
in the outer scope. Three mechanisms, in order of increasing explicitness:

**Names-match (default):** variables with the same name are automatically identified.

```evident
within_budget    -- outer 'assignments' and 'budget' match sub-claim variables by name
```

**Named mapping with `↦`:** rename variables that differ. Multi-line for many remappings:

```evident
within_budget
    assignments ↦ team_members
    budget      ↦ project_limit
```

**Pass-through with `..`:** unmatched sub-claim variables lift up into the parent's
variable set. Opt-in only — not the default:

```evident
within_budget ..              -- unmatched vars lift up
within_budget ..
    budget ↦ project_limit    -- lift, but rename budget
```

Without `..`, all sub-claim variables must already exist in the outer scope.
This keeps the parent claim's interface visible from its declaration.

---

## Rule 10: Prefer universal statements over case analysis

Multiple `evident` blocks for the same claim express disjunction — "holds when A *or* when B." Before writing separate cases, ask: can a single universal statement cover all of them?

**Base cases are usually vacuously true.** A universal `∀` over an empty collection holds automatically — the solver handles it without being told. You do not need to write a base case for the empty list, the zero value, or the trivial instance.

```evident
-- Wrong: explicit base cases
evident sorted []
evident sorted [_]
evident sorted [a, b | rest] when a ≤ b
    sorted [b | rest]

-- Right: one universal statement; empty and singleton cases are vacuous
evident sorted list
    ∀ (a, b) ∈ each_consecutive list : a ≤ b
```

**Use forward implications for closure properties.** Reflexivity, transitivity, symmetry, and other closure properties are naturally expressed as `⇒` rules, not as base cases plus recursion.

```evident
-- Wrong: base case + recursive case
evident reachable a a
evident reachable a c
    adjacent a _b
    reachable _b c

-- Right: two closure properties; the solver derives the transitive closure
node n ⇒ reachable n n
reachable a b, adjacent b c ⇒ reachable a c
```

**When are multiple evident blocks justified?** When the argument genuinely has structurally distinct variants — different constructors of an algebraic type — and no uniform statement covers all of them. Primitive list operations (`first_of`, `last_of`, `each_consecutive`) have multiple clauses because lists have distinct structural forms. Higher-level claims built on those primitives should not.

The test: if your second `evident` block is a "base case" that would be vacuously true under a universal formulation, delete it and write the universal instead.

---

## Summary

| Avoid | Prefer |
|---|---|
| Action verb names (`find_`, `get_`, `compute_`) | State/relationship names (`valid`, `_of`, `_in`, `_for`) |
| Named lookup claims | Inline existential with `?x in collection, constraint` |
| Thinking about order of evaluation | Thinking about simultaneous constraints |
| Names that describe procedures | Names that describe facts |
| "What does this claim do?" | "What must be true for this claim to hold?" |
