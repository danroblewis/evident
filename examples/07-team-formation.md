# Example 7: Team Formation — Composing Constraint Sub-Systems

A project needs a team. People have skills and salaries. Roles have skill
requirements. A budget exists. The solver finds who fills which role.

This example composes four sub-systems into one. Each sub-system is a claim.
The top-level claim references all four. Self-evident data goes at the bottom;
the solver populates the unbound `assignments` variable.

---

## Types

```evident
type Skill = String

type Person = {
    name    ∈ String
    skills  ∈ Set Skill
    salary  ∈ Nat
}

type Role = {
    title           ∈ String
    required_skills ∈ Set Skill
}

type Assignment = {
    person ∈ Person
    role   ∈ Role
}
```

---

## Sub-system 1: Qualification

A person qualifies for a role when their skills cover all of the role's requirements.

```evident
claim qualifies
    person ∈ Person
    role   ∈ Role
    person.skills ⊇ role.required_skills
```

---

## Sub-system 2: Role coverage

Every required role has exactly one person assigned to it.

```evident
claim roles_covered
    roles       ∈ Set Role
    assignments ∈ Set Assignment
    ∀ role ∈ roles : ∃ a ∈ assignments : a.role = role
```

---

## Sub-system 3: Budget

The sum of assigned salaries fits within the budget.

```evident
claim within_budget
    assignments ∈ Set Assignment
    budget      ∈ Nat
    _total = sum { a.person.salary | a ∈ assignments }
    _total ≤ budget
```

---

## Sub-system 4: No person assigned twice

Each person appears in at most one assignment.

```evident
claim no_double_assignment
    assignments ∈ Set Assignment
    ∀ a ∈ assignments, ∀ b ∈ assignments :
        a ≠ b ⇒ a.person ≠ b.person
```

---

## Top-level: valid team

Composing all four sub-systems. Each body line is either a type constraint
or an invocation of a sub-system — all are constraints, all must hold simultaneously.

```evident
claim valid_team
    roles       ∈ Set Role
    candidates  ∈ Set Person
    budget      ∈ Nat
    assignments ∈ Set Assignment
    -- every assignment uses a candidate and a required role
    ∀ a ∈ assignments : a.person ∈ candidates
    ∀ a ∈ assignments : a.role ∈ roles
    -- every assignment: person is qualified for their role
    ∀ a ∈ assignments : qualifies a.person a.role
    -- all roles are filled
    roles_covered roles assignments
    -- total cost is within budget
    within_budget assignments budget
    -- no person assigned twice
    no_double_assignment assignments
```

---

## Self-evident data

```evident
assert python  ∈ Skill
assert ml      ∈ Skill
assert java    ∈ Skill
assert backend ∈ Skill
assert data    ∈ Skill

assert alice = { name = "Alice", skills = { python, ml, data }, salary = 85000 }
assert bob   = { name = "Bob",   skills = { java, backend },    salary = 75000 }
assert carol = { name = "Carol", skills = { python, data, ml }, salary = 80000 }
assert dan   = { name = "Dan",   skills = { java, backend, ml }, salary = 90000 }

assert ml_engineer      = { title = "ML Engineer",      required_skills = { python, ml } }
assert backend_engineer = { title = "Backend Engineer", required_skills = { java, backend } }
assert data_scientist   = { title = "Data Scientist",  required_skills = { python, data } }

assert project_roles      = { ml_engineer, backend_engineer, data_scientist }
assert project_candidates = { alice, bob, carol, dan }
assert project_budget     = 250000
```

---

## Query: find a valid team

`assignments` is unbound. The solver fills it in.

```evident
valid_team project_roles project_candidates project_budget assignments
```

```
-- Solver returns (one valid assignment):
assignments = {
    { person = alice, role = ml_engineer }
    { person = bob,   role = backend_engineer }
    { person = carol, role = data_scientist }
}
-- Total salary: 85000 + 75000 + 80000 = 240000 ≤ 250000 ✓
-- Alice: { python, ml, data } ⊇ { python, ml } ✓
-- Bob:   { java, backend }    ⊇ { java, backend } ✓
-- Carol: { python, data, ml } ⊇ { python, data } ✓
```

```
-- Alternative valid assignment also exists:
assignments = {
    { person = carol, role = ml_engineer }
    { person = bob,   role = backend_engineer }
    { person = alice, role = data_scientist }
}
-- Total: 80000 + 75000 + 85000 = 240000 ≤ 250000 ✓
-- All qualifications hold ✓
```

```
-- Dan would push over budget with either Alice or Carol:
-- Dan + Bob + Alice = 90000 + 75000 + 85000 = 250000 ≤ 250000 ✓ (exactly at limit)
-- Dan + Bob + Carol = 90000 + 75000 + 80000 = 245000 ≤ 250000 ✓
-- So Dan is also a valid candidate in some assignments.
```

---

## Querying the sub-systems independently

Because each sub-system is a named claim, it can be queried on its own.

```evident
-- Does Alice qualify for the ML engineer role?
qualifies alice ml_engineer
-- Yes ✓

-- Does Bob qualify for the data scientist role?
qualifies bob data_scientist
-- No  (Bob has { java, backend }, missing { python, data })

-- Which candidates qualify for the backend role?
∃ person ∈ project_candidates : qualifies person backend_engineer
-- bob, dan

-- What is the cheapest valid assignment?
valid_team project_roles project_candidates project_budget assignments
    minimizing sum { a.person.salary | a ∈ assignments }
-- assignments = { alice/ml, bob/backend, carol/data }  total = 240000
```

---

## What the sub-systems contributed

| Sub-system | What it ruled out |
|---|---|
| `qualifies` | Assignments where the person lacks required skills |
| `roles_covered` | Assignments that leave any role unfilled |
| `within_budget` | Teams whose total salary exceeds the budget |
| `no_double_assignment` | Assignments that give one person two roles |

Each sub-system narrows the solution space. The intersection of all four is
the set of valid teams. The solver finds members of that intersection.
The sub-systems can be reused, composed differently, or queried independently.
