# Evident Specification — Claims

## Overview

A claim is a named constraint system. The `claim` keyword introduces the name.
Everything indented below is a constraint on the claim's variables. All constraints
hold simultaneously — there is no ordering, no execution sequence, no steps.

The solver finds values for any unbound variables such that all constraints are
simultaneously satisfied. A claim does not compute a result. It names a set of
satisfying assignments.

---

## Claim declarations

```evident
claim sorted
    T    ∈ Ordered
    list ∈ List T
    ∀ (a, b) ∈ each_consecutive list : a ≤ b
```

The claim head is the name: `sorted`. Everything indented below it is the body.
The body is a flat list of constraints — every line states something that must hold.
There is no distinction between "the parameter list" and "the body." All lines are
constraints. They are all satisfied simultaneously.

`T ∈ Ordered` constrains T to be a type with an ordering. `list ∈ List T`
constrains `list` to be a list of T elements. The final line constrains every
consecutive pair in the list to be non-decreasing.

The set named `sorted` is the set of all (T, list) pairs satisfying every
constraint in the body.

---

## Variables

Every named occurrence in a claim body is a variable. Named variables (no `_`
prefix) are the claim's interface — accessible from outside by name. Variables with
a `_` prefix are internal scaffolding.

### Interface variables

```evident
claim path_between
    g    ∈ Graph
    a    ∈ g.nodes
    b    ∈ g.nodes
    path ∈ List g.nodes
    -- constraints on path go here
```

`g`, `a`, `b`, `path` are interface variables. Callers can bind any of them from
outside. The solver finds values for whichever are left unbound.

### Body-internal variables

```evident
evident occurrences x [x | rest] n
    _n0 = occurrences x rest
    n   = _n0 + 1
```

`_n0` is body-internal. It has no domain meaning — it is implementation scaffolding.
The solver finds its value. The underscore prefix signals this to readers.

Body-only names without `_` are also implicitly existential (the solver finds them),
but the underscore convention communicates "this has no meaningful domain name."

### Type parameters

Type parameters use `[...]` after the claim name:

```evident
claim sorted[T ∈ Ordered]
    list ∈ List T
    ∀ (a, b) ∈ each_consecutive list : a ≤ b
```

`T` ranges over types in the `Ordered` class. It is bound when the claim is applied
to a concrete list type.

### Variable scope rule

**Head names** (named variables in interface position) are bound from outside.
**Body-only names** are found by the solver — they are implicitly existential.
The rule is: if a name does not appear in the claim head, its value is determined
by the solver.

---

## Body constraints

Body lines are constraint conditions. All hold simultaneously. The solver does not
process them in order — it finds a consistent assignment for all of them at once.

### Type membership

```evident
x ∈ Nat
list ∈ List T
g ∈ Graph
```

Constrains `x` to be a natural number, `list` to be a list of T, `g` to be a graph.
These are the same kind of constraint as any other body line — membership conditions
narrowing the solution space.

### Arithmetic

```evident
a + b = c
n ≤ 10
a.duration ≤ a.slot.end - a.slot.start
```

Standard arithmetic relations. `=` here is constraint equality — both sides must
be equal in any solution. It is not assignment.

### Logical connectives

```evident
condition ∨ other_condition
¬ suspended
a.verified ∧ ¬ a.suspended
```

`∨` (or), `∧` (and), `¬` (not). The solver must find values satisfying whatever
logical combination is expressed.

### Claim application

```evident
sorted list
assignment_valid a
```

Invoking a claim in a body means: merge this constraint system with the sub-claim's
constraint system by identifying shared variable names. `sorted list` posts all the
constraints of `sorted` into the current system, with `list` identified to the
sub-claim's `list` variable. No function call, no return value. Just constraint
composition.

See the composition section for details.

### Quantifiers

```evident
∀ (a, b) ∈ each_consecutive list : a ≤ b
∀ t ∈ tasks : deadline_met t schedule
∃ p ∈ paths : path_length p d
```

`∀ x ∈ S : P(x)` — for every element of S, constraint P must hold. The solver
grounds this to a concrete constraint per element before solving.

`∃ x ∈ S : P(x)` — there exists at least one element of S satisfying P. Without
an explicit `∃`, body-level free variables are already implicitly existential. Use
`∃` when you need to name the witness or scope it:

```evident
∃ _path ∈ List g.nodes :
    path_between g a b _path
    path_length _path d
```

### Set operations and comprehensions

```evident
S ⊆ T
x ∈ S ∩ T
{ a.room | a ∈ schedule, a.slot = slot }
schedule[.slot = slot]
```

Standard set algebra. The filter notation `S[condition]` is shorthand for
`{ a ∈ S | condition(a) }`. Field projection `S.field` is `{ a.field | a ∈ S }`.

---

## det claim binding

When a claim is deterministic — exactly one solution for given inputs — its result
can be bound with `=`:

```evident
_len = length list    -- _len is constrained to equal the unique length of list
n    = _len + 1
```

The `=` on the left is constraint equality. `_len` is constrained to equal the
result of `length list`. Because `length` is `det`, there is exactly one value
satisfying this constraint. The solver uses substitution rather than search.

This is a shorthand for `length list _len` when `length` has exactly one solution
per input. If `length` were `nondet`, this form would be ill-typed.

Compare:

```evident
-- det: bind with =
_len = length list

-- nondet or semidet: apply as constraint (no =)
divisible n 3    -- no unique result; just states the relationship
```

Note: the `=` binding form implies directionality that doesn't perfectly match
Evident's relational model. This notation is pragmatically useful for `det`
claims but is under reconsideration. See open questions.

---

## Multiple evident blocks (structural recursion)

Use separate `evident` blocks only for genuinely distinct structural cases — when
the argument has different constructors and no single universal statement covers all
of them:

```evident
evident occurrences x [] 0

evident occurrences x [x | rest] n
    _n0 = occurrences x rest
    n   = _n0 + 1

evident occurrences x [y | rest] n when x ≠ y
    n = occurrences x rest
```

Each `evident` block covers one structural case. The three blocks together define
`occurrences` by case analysis on the list.

When a single universal statement works instead, prefer it. The `∀` formulation
does not require a base case because the universal over an empty collection is
vacuously true:

```evident
-- prefer this:
claim sorted[T ∈ Ordered]
    list ∈ List T
    ∀ (a, b) ∈ each_consecutive list : a ≤ b

-- over this:
evident sorted []           -- base case: unnecessary, vacuously true
evident sorted [_]          -- base case: unnecessary, vacuously true
evident sorted [a, b | rest] when a ≤ b
    sorted [b | rest]
```

The test: if the second `evident` block is a base case that would be vacuously true
under a universal formulation, delete it and write the universal instead.

---

## Guards

A `when` condition on a claim head restricts when the block applies:

```evident
evident sorted [a, b | rest] when a ≤ b
    sorted [b | rest]
```

This block applies only when `a ≤ b` holds. If it does not, this block is not a
candidate for satisfying `sorted`. Multiple `evident` blocks with different guards
express disjunction — the claim holds when at least one block's guard and body are
satisfied.

Guards can include any constraint expression:

```evident
evident schedule_fits assignment slot when assignment.duration ≤ slot.length
    assignment.start ∈ slot.valid_starts
```

---

## Forward implications (top-level rules)

Forward implications define derived facts that fire when their preconditions are
established in the evidence base:

```evident
node n ⇒ reachable n n
reachable a b, adjacent b c ⇒ reachable a c
```

These are not claims — they are top-level rules. When the left side is established
(by assertion or derivation), the right side is automatically added to the evidence
base. They run to fixpoint: the second rule fires repeatedly until no new `reachable`
facts can be derived.

Forward implications are the right tool for:

- **Reflexivity**: `node n ⇒ reachable n n`
- **Transitivity**: `reachable a b, adjacent b c ⇒ reachable a c`
- **Derived membership**: `employee e, e.department = d ⇒ member_of e d`
- **Closure properties**: any relation defined by propagation from base cases

Compare to claims, which the solver satisfies on demand. Forward implications are
proactive — they fire automatically as facts accumulate.

```evident
-- Reachability (forward implications — transitive closure)
node n ⇒ reachable n n
reachable a b, adjacent b c ⇒ reachable a c

-- Query on demand (claim applied to specific values)
claim shortest_path
    g ∈ Graph
    a ∈ g.nodes
    b ∈ g.nodes
    d ∈ Nat
    reachable g a b
    -- (solver finds minimal d)
```

---

## Nested claims (constraint modules)

When several claims share the same set of variables, they can be grouped under a
parent claim. Inner claims inherit the outer claim's variable scope.

```evident
claim Conference
    schedule ∈ Set Assignment
    talks    ∈ Set Talk
    rooms    ∈ Set Room
    slots    ∈ Set Slot

    claim rooms_conflict_free
        -- 'schedule' is in scope from the outer claim
        ∀ slot ∈ { a.slot | a ∈ schedule } :
            all_different { a.room | a ∈ schedule, a.slot = slot }

    claim speakers_conflict_free
        ∀ slot ∈ { a.slot | a ∈ schedule } :
            all_different { a.talk.speaker | a ∈ schedule, a.slot = slot }

    claim valid
        rooms_conflict_free
        speakers_conflict_free
        ∀ a ∈ schedule : assignment_valid a
```

`Conference` declares the shared variables. Inner claims `rooms_conflict_free`,
`speakers_conflict_free`, and `valid` can reference those variables directly —
`schedule`, `talks`, `rooms`, `slots` are in scope without re-declaration.

This is not a class. A nested claim block is a **constraint namespace** — shared
variables with claims that constrain them. There is no stored state, no mutation,
no methods.

### Accessing sub-claims from outside

```evident
Conference.valid             -- the full validity constraint
Conference.rooms_conflict_free  -- a specific sub-constraint
```

### Using the block by names-match

When the outer scope has variables with matching names, they flow automatically:

```evident
claim manage_event
    schedule ∈ Set Assignment   -- matches Conference.schedule by name
    talks    ∈ Set Talk
    rooms    ∈ Set Room
    slots    ∈ Set Slot
    Conference.valid            -- all variables flow by names-match
```

### Pass-through with `..`

To lift the block's variables into the outer claim's scope:

```evident
    Conference ..    -- all of Conference's variables become part of this claim
```

---

## Claim composition

When a sub-claim is invoked in a body, its variables are identified with variables
in the outer scope. Three mechanisms in order of increasing explicitness:

### Names-match (default)

Variables with the same name are automatically identified. No explicit argument
syntax needed:

```evident
claim valid_conference
    talks    ∈ Set Talk
    rooms    ∈ Set Room
    slots    ∈ Set Slot
    schedule ∈ Set Assignment

    all_talks_scheduled      -- 'talks' and 'schedule' match by name
    rooms_conflict_free      -- 'schedule' matches
    ∀ a ∈ schedule : assignment_valid a
```

### Named mapping with `↦`

When variable names differ, map them explicitly:

```evident
rooms_conflict_free schedule ↦ team_a_schedule

-- Or multi-line for several remappings:
within_budget
    assignments ↦ team_members
    budget      ↦ project_limit
```

Variables not listed in the mapping are still matched by name if possible.

### Filling specific slots with `:`

When invoking a claim with specific values or when applying a claim to a different
variable than the name suggests:

```evident
active_editor user: resource.owner    -- resource.owner fills the 'user' slot
recently_active days: 30              -- fix 'days', leave other variables free
```

---

## Partial application

Fix some variables of a claim, leave others free, to name a new (narrower) claim:

```evident
claim editor    = has_role role: "editor"    -- 'user' still free
claim admin     = has_role role: "admin"     -- 'user' still free
claim in_acme   = within_org org_id: 42     -- 'user' still free
```

`editor` is now a one-variable claim: `user ∈ editor` holds when the user has the
editor role. The fixed variable (`role = "editor"`) is baked in.

Partial application is useful for:

- **Named policy elements**: `claim editor = has_role role: "editor"`
- **Org-scoped constraints**: `claim acme_user = within_org org_id: 42`
- **Time-bounded constraints**: `claim active_30_days = recently_active days: 30`

Partially applied claims compose with `·` (constraint intersection):

```evident
claim active_editor = active_account · email_verified · editor
claim acme_editor   = active_account · email_verified · editor · in_acme
```

`x ∈ active_editor` holds when x satisfies `active_account`, `email_verified`,
and `editor` simultaneously. The `·` operator is constraint intersection — each term
narrows the solution space.

---

## Determinism annotations

Claims can be annotated with their determinism — how many solutions exist for a
given set of inputs:

- `det` — exactly one solution (functions)
- `semidet` — zero or one solution (partial functions, decisions)
- `nondet` — zero or more solutions (relations, generators)

These are hints to the solver, not enforced by the type system. They affect how
the solver approaches a claim and which syntactic forms are valid at call sites.

```evident
claim length[T]          det
    list ∈ List T
    n    ∈ Nat
    -- exactly one n for any list

claim prime              semidet
    n ∈ Nat
    -- n is prime or it isn't; no choice involved

claim permutation_of[T]  nondet
    xs ∈ List T
    ys ∈ List T
    -- many permutations may exist
```

A `det` claim can use the `= claim args` binding shorthand in bodies:

```evident
_len = length list    -- valid only because length is det
```

A `nondet` claim cannot use this shorthand — there may be multiple results, so
binding to one of them requires explicit existential quantification:

```evident
∃ _perm ∈ List T : permutation_of xs _perm
```

### Annotation syntax

The annotation follows the claim name and type parameters, before the body:

```evident
claim length[T]    det
    list ∈ List T
    n    ∈ Nat

claim divisible    semidet
    a ∈ Nat
    b ∈ Nat
    ∃ k ∈ Nat : a = b * k
```

---

## Complete examples

### Sorting

```evident
claim sorted[T ∈ Ordered]
    list ∈ List T
    ∀ (a, b) ∈ each_consecutive list : a ≤ b

claim sorted_permutation_of[T ∈ Ordered]
    xs ∈ List T
    ys ∈ List T
    sorted ys
    permutation_of xs ys
```

### Graph reachability (forward implications)

```evident
-- Ground facts
assert node : Nat → Prop
assert adjacent : Nat → Nat → Prop

-- Closure rules
node n ⇒ reachable n n
reachable a b, adjacent b c ⇒ reachable a c

-- Query (claim over specific values)
claim connected
    g ∈ Graph
    ∀ a ∈ g.nodes, ∀ b ∈ g.nodes : reachable a b
```

### Occurrences (structural recursion)

```evident
evident occurrences x [] 0

evident occurrences x [x | rest] n
    _n0 = occurrences x rest
    n   = _n0 + 1

evident occurrences x [y | rest] n when x ≠ y
    n = occurrences x rest
```

### Access control with partial application and chaining

```evident
claim has_role
    user ∈ User
    role ∈ Role
    role ∈ user.roles

claim active_account
    user ∈ User
    ¬ user.suspended

claim email_verified
    user ∈ User
    user.verified

-- Partial application
claim editor = has_role role: "editor"
claim admin  = has_role role: "admin"

-- Constraint intersection
claim can_edit
    user     ∈ User
    resource ∈ Resource
    user ∈ active_account · email_verified · editor
    resource.org_id = user.org_id
```

### Conference scheduling (nested claims)

```evident
claim Conference
    schedule ∈ Set Assignment
    talks    ∈ Set Talk
    rooms    ∈ Set Room
    slots    ∈ Set Slot

    claim rooms_conflict_free
        ∀ slot ∈ { a.slot | a ∈ schedule } :
            all_different { a.room | a ∈ schedule, a.slot = slot }

    claim all_talks_covered
        ∀ talk ∈ talks :
            exactly 1 { a ∈ schedule | a.talk = talk }

    claim valid
        rooms_conflict_free
        all_talks_covered
        ∀ a ∈ schedule : assignment_valid a
```

---

## Open questions

### Determinism annotation syntax

The position and form of determinism annotations are not settled.

Option A — trailing annotation before the body:
```evident
claim length[T]    det
    list ∈ List T
    n    ∈ Nat
```

Option B — as a type-like annotation:
```evident
claim length[T] : det
    list ∈ List T
    n    ∈ Nat
```

Option C — explicit arity annotation:
```evident
claim length[T] (list ∈ List T) → (n ∈ Nat)    -- det implied by single →
```

Option C risks reintroducing functional notation, which conflicts with Evident's
relational model. Options A and B are more consistent with the constraint-first view.

### The `=` form for det claims

`_len = length list` implies directionality (function application, assignment) that
does not match the relational semantics of Evident. The notation is pragmatically
convenient but conceptually wrong. Alternatives:

- Positional: `length list _len` — honest (constraint joining), but looks like a function call
- Named: `with length: list = list, n = _len` — explicit but verbose
- Inline existential: `∃ _len ∈ Nat : length list _len` — fully explicit

No syntax is settled. The current examples use `= claim args` as a placeholder.

### Composition operator for constraint intersection

The `·` (middle dot) is used in examples for constraint chaining (`A · B · C`).
The alternative `⋈` (bowtie) is semantically more accurate — it means natural join,
which is exactly what constraint composition is. Neither is committed.

### Forward implication scope

It is not settled whether forward implications can appear inside a claim body (local
rules) or only at the top level. Top-level-only is the current assumption; local
rules would add complexity to the solver's grounding step.
