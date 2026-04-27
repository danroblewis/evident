# Evident Specification — Assertions and Queries

## Ground fact assertions

Ground facts establish concrete values in the evidence base. The solver treats
them as fixed; they are not subject to search.

```evident
assert x = 5
assert name = "Alice"
assert edge 1 2
assert tasks = { task_a, task_b, task_c }
assert config = { host = "localhost", port = 5432 }
```

Assertions with `=` bind a variable to a value. Assertions without `=` assert
that a relation holds for given arguments (as with `edge 1 2`).

## Unbound variable declaration

Declare that a variable exists without giving it a value. The solver is
responsible for finding a value consistent with all constraints.

```evident
assert result ∈ Nat             -- result exists; solver determines its value
assert schedule ∈ Set Assignment   -- unbound set; solver determines its elements
```

An unbound variable with a type constraint tells the solver the domain to search
in. Without additional constraints the solver may return any value in the domain.

## Applying a constraint

Name a claim in the body of a program or at top level to require that it holds.
The solver must find values for all unbound variables that satisfy it.

```evident
valid_conference    -- solver must establish this; finds values for unbound variables
sorted list        -- constrain: list must be sorted (list may be bound or unbound)
```

This is the primary way to state a goal. The claim acts as a constraint on the
current variable environment.

## Queries

Queries ask the solver a question without asserting anything permanently.

```evident
? claim                         -- is this claim established?
? can_edit alice document_42    -- check: does this relation hold?
? ∃ x ∈ S : P(x)              -- find some x in S satisfying P
? ∀ x ∈ S : P(x)              -- verify P holds for every x in S
```

A `?` expression does not modify the evidence base. It interrogates it.

## Bidirectionality

The same claim can be queried in multiple modes depending on which variables are
bound. The solver handles all modes uniformly.

```evident
? can_edit alice document_42           -- check: is the relation established?
? ∃ user ∈ User : can_edit user doc   -- find: who can edit this document?
? ∃ doc ∈ Resource : can_edit alice doc -- find: what can Alice edit?
```

There is no separate predicate for each direction. The relational interpretation
of `can_edit` makes all three queries expressions of the same constraint system.

## Solver behavior

When constraints are satisfiable:

- The solver returns a **witness**: an assignment of values to all unbound
  variables that satisfies every constraint
- The solver also returns the **evidence term**: the derivation tree showing how
  the claim was established
- The evidence term is a first-class value and can be stored, inspected, or
  passed to other claims

When constraints are unsatisfiable:

- The solver reports which constraints conflict
- Exact error model is not yet defined. See open questions.

When constraints are underconstrained (multiple solutions exist):

- By default the solver returns one valid assignment
- The query form `? ∀ x ∈ S : P(x)` verifies that all elements satisfy the
  condition; it does not enumerate
- Enumeration of all solutions is supported but the syntax is not yet committed

## Evidence terms

When a claim is established, the derivation is available as a value.

```evident
assert ev = evidence valid_conference
```

Evidence terms record which constraints were applied and with what witnesses.
They support auditing, debugging, and claims that reason about other claims
(meta-reasoning). The full structure of evidence terms is specified separately.

## Open questions

- Error model for unsatisfiable constraints: what does the solver return, and in
  what form is the conflict reported?
- Syntax for requesting all solutions rather than one
- Whether queries inside claim bodies (as sub-goals) are syntactically distinct
  from top-level queries
- Evidence term structure: fields, access syntax, whether evidence is typed
