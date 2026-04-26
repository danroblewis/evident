# Example 7: Team Formation

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


claim qualifies
    person ∈ Person
    role   ∈ Role
    person.skills ⊇ role.required_skills


claim roles_covered
    roles       ∈ Set Role
    assignments ∈ Set Assignment
    ∀ role ∈ roles : ∃ a ∈ assignments : a.role = role


claim within_budget
    assignments ∈ Set Assignment
    budget      ∈ Nat
    _total = sum { a.person.salary | a ∈ assignments }
    _total ≤ budget


claim no_double_assignment
    assignments ∈ Set Assignment
    ∀ a ∈ assignments, ∀ b ∈ assignments :
        a ≠ b ⇒ a.person ≠ b.person


claim valid_team
    roles       ∈ Set Role
    candidates  ∈ Set Person
    budget      ∈ Nat
    assignments ∈ Set Assignment
    ∀ a ∈ assignments :
        a.person ∈ candidates
        a.role   ∈ roles
        qualifies a.person a.role
    roles_covered roles assignments
    within_budget assignments budget
    no_double_assignment assignments


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
assert data_scientist   = { title = "Data Scientist",   required_skills = { python, data } }

assert project_roles      = { ml_engineer, backend_engineer, data_scientist }
assert project_candidates = { alice, bob, carol, dan }
assert project_budget     = 250000


valid_team project_roles project_candidates project_budget assignments
```
