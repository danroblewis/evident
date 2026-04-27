# Example 15: Access Control — Constraint Chaining and Partial Application

Access control is the clearest fit for constraint chaining. Every condition is a named,
independent concept. The chain reads like a policy document. Each term narrows the set
of users who are allowed to proceed.

The composition operator used here is `·` (middle dot). It means: x must be in all of
these constraint sets simultaneously. Each term is a named constraint set; their
intersection is the policy.

---

## Types

```evident
type Role = String

type User = {
    id             ∈ Nat
    email          ∈ String
    roles          ⊆ Role
    verified       ∈ Bool
    suspended      ∈ Bool
    org_id         ∈ Nat
    last_active_at ∈ Nat    -- unix timestamp
}

type Session = {
    user       ∈ User
    created_at ∈ Nat
    expires_at ∈ Nat
}

type Resource = {
    id     ∈ Nat
    org_id ∈ Nat
    owner  ∈ User
}
```

---

## Sub-claim definitions

```evident
-- Account is not suspended
claim active_account
    user ∈ User
    ¬ user.suspended

-- Email has been verified
claim email_verified
    user ∈ User
    user.verified

-- User has a specific role
claim has_role
    user ∈ User
    role ∈ Role
    role ∈ user.roles

-- User belongs to a specific org
claim within_org
    user   ∈ User
    org_id ∈ Nat
    user.org_id = org_id

-- Session has not expired
claim session_valid
    session      ∈ Session
    current_time ∈ Nat
    session.expires_at > current_time

-- User has been active recently (within n days)
claim recently_active
    user         ∈ User
    current_time ∈ Nat
    days         ∈ Nat
    current_time - user.last_active_at < days * 86400

-- Resource belongs to the user's org
claim resource_in_org
    user     ∈ User
    resource ∈ Resource
    resource.org_id = user.org_id
```

---

## Partial application — naming partially applied constraints

`has_role` takes two variables: `user` and `role`. Fixing `role` to a specific string
gives a new 1-variable constraint — a named policy element.

```evident
-- Partial application: fix 'role', leave 'user' free
claim editor    = has_role role: "editor"
claim admin     = has_role role: "admin"
claim viewer    = has_role role: "viewer"
claim moderator = has_role role: "moderator"

-- Partial application with multiple fixed variables
claim active_editor = active_account · email_verified · editor
claim active_admin  = active_account · email_verified · admin
```

`editor` is now a 1-variable constraint: `user ∈ editor` means the user has the editor
role. `active_editor` is a named intersection of three constraints.

Similarly for `recently_active`:

```evident
-- Fix 'days', leave 'user' and 'current_time' free
claim active_30_days  = recently_active days: 30
claim active_90_days  = recently_active days: 90
```

And for `within_org` — useful when org_id is known from context:

```evident
-- Fix org_id to a specific org
claim in_acme_org = within_org org_id: 42
```

---

## Policies using constraint chaining

```evident
-- Can view any content in their org
claim can_view
    user     ∈ User
    resource ∈ Resource
    user ∈ active_account · email_verified · resource_in_org resource

-- Can edit content they have access to
claim can_edit
    user     ∈ User
    resource ∈ Resource
    user ∈ active_account · email_verified · editor · resource_in_org resource

-- Can publish (editors who have been recently active)
claim can_publish
    user         ∈ User
    resource     ∈ Resource
    current_time ∈ Nat
    user ∈ active_account · email_verified · editor · resource_in_org resource
         · recently_active days: 30

-- Admin operations require more conditions
claim can_admin
    user   ∈ User
    org_id ∈ Nat
    user ∈ active_account · email_verified · admin · within_org org_id

-- Session-gated actions (user AND session must be valid)
claim authenticated_action
    user         ∈ User
    session      ∈ Session
    current_time ∈ Nat
    user    ∈ active_account · email_verified
    session ∈ session_valid
    session.user = user
```

---

## Partially applied policies

```evident
-- A policy for a specific org (org_id fixed)
claim acme_editor = active_account · email_verified · editor · in_acme_org

-- Usage: is this user an acme editor?
assert user ∈ acme_editor
```

---

## Adding a new requirement is one line

The power of the chaining model: policy changes are additive. If the security team
decides all editors must also have 2FA enabled:

```evident
-- New sub-claim:
claim two_factor_enabled
    user ∈ User
    user.two_factor ∈ Bool
    user.two_factor

-- Update can_edit by adding one term:
claim can_edit
    user     ∈ User
    resource ∈ Resource
    user ∈ active_account · email_verified · editor · two_factor_enabled · resource_in_org resource
```

One term added. No refactoring of conditions. No boolean logic to untangle.

---

## Querying the policy

```evident
-- Is this user allowed to edit this resource?
? can_edit alice document_42

-- Which users can edit this resource?
? ∃ user ∈ User : can_edit user document_42

-- Which resources can Alice edit?
? ∃ resource ∈ Resource : can_edit alice resource

-- The same claim, three directions. No separate "check", "find", or "list" functions.
```
