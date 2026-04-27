# Evident Specification — Composition

## The composition model

Composing two constraint systems means identifying shared variables — merging
them into one system where all constraints hold simultaneously.

Variables with the same name across both systems become the same variable. The
result is their natural join on shared variable names.

This is **natural join** from relational algebra, or equivalently **pullback**
from category theory: the universal construction that equates shared dimensions
and keeps everything else independent.

```evident
claim rooms_conflict_free
    schedule ∈ Set Assignment

claim parallel_load_within
    schedule ∈ Set Assignment
    slots    ∈ Set Slot

-- Composing: schedule is shared, identified automatically.
-- The composed system has: schedule, slots, and all constraints from both.
```

## Names-match (default)

When a sub-claim is referenced in a body, variables with matching names across
the two systems are automatically identified. No annotation required.

```evident
claim valid_conference
    schedule ∈ Set Assignment
    slots    ∈ Set Slot

    rooms_conflict_free    -- 'schedule' flows by names-match
    parallel_load_within   -- 'schedule' and 'slots' flow by names-match
```

The name `schedule` in `valid_conference` and the name `schedule` in
`rooms_conflict_free` denote the same variable. The solver sees one combined
system.

## Explicit mapping with `:`

When variable names differ, map them explicitly. Syntax: `sub_claim variable: local_value`

```evident
-- single variable
active_editor user: resource.owner

-- multiple variables
jwt_authenticated principal: user, token: auth_token
```

The name left of `:` is the sub-claim's variable. The value right of `:` is
from the current scope. The sub-claim's variable is unified with the local value.

## Block mapping with `↦` (mapsto)

For many remappings, use a block form. Type `mapsto` and the editor shows `↦`.
Each line: `sub_claim_variable ↦ local_value`.

```evident
my_big_claim(
    user    ↦ admin_user
    age     ↦ admin_years
    org_id  ↦ target_org
)
```

Inline and block mapping are equivalent. Block form is preferred when three or
more variables are remapped.

## Pass-through (`..`)

Lift all variables of a sub-claim into the current scope:

```evident
claim dag
    ..graph                         -- lifts nodes, edges, and all graph constraints
    ∀ x ∈ nodes : ¬ reachable edges x x
```

Variables from the sub-claim become variables of the enclosing claim. Unmatched
variables lift through unchanged. This makes all of the sub-claim's structure
directly available without naming it.

Combine pass-through with explicit mapping:

```evident
within_budget ..
    budget ↦ project_limit    -- rename budget; other variables lift through
```

Variables that are remapped use the new name in the current scope. All others
lift with their original names.

## Partial application

Fix some variables of a claim to create a new named constraint:

```evident
claim editor    = has_role role: "editor"
claim in_acme   = within_org org_id: 42
claim active_30 = recently_active days: 30
```

The result is a claim with the fixed variables eliminated. Remaining free
variables are still parameters of the new claim.

```evident
-- has_role has variables: user, role
-- editor fixes role = "editor"
-- editor has one remaining variable: user

? can_edit alice doc
    alice ∈ editor    -- role is already fixed; user unifies with alice
```

## Constraint chain composition

Chain multiple constraint sets with `·` (middle dot) or `⋈` (bowtie). Each
term is a set; the chain is their intersection.

```evident
user ∈ active_account · email_verified · editor · within_org
```

The variable `user` flows through all terms by names-match. The constraint
requires `user` to be simultaneously in every set in the chain.

Multiline form, when the chain is long:

```evident
x ∈ (
    lessthan 50
    · positive
    · greater 5
)
```

The chain operators `·` and `⋈` are synonyms. The choice between them is a
matter of convention and is not yet committed. See open questions.

## Nested claims (constraint modules)

Claims can be nested. Inner claims have access to variables in scope from the
enclosing claim.

```evident
claim Conference
    schedule ∈ Set Assignment

    claim rooms_conflict_free
        -- 'schedule' is in scope from Conference
        ∀ (a, b) ∈ schedule × schedule :
            a ≠ b ⇒ ¬ (a.room = b.room ∧ a.slot = b.slot)

    claim valid
        rooms_conflict_free
```

Sub-claims are accessed with `.` notation: `Conference.valid`,
`Conference.rooms_conflict_free`.

Nested claims are constraint modules — they group related constraints and give
them names, but they are not types or objects. They are still claims: sets
defined by membership conditions.

## Open questions

- Chain operator: `·` (middle dot) vs `⋈` (bowtie) — not yet committed; both
  are reserved
- Whether block mapping syntax (`(...)`) conflicts with grouping syntax
- Semantics of pass-through when sub-claim variables shadow outer variables
