# Evident Syntax Reference

Quick reference for the syntax used in the examples. These are working decisions, not final specifications.

## Claims

```evident
-- Declare a claim family and its type
claim sorted[T : Ordered] : List T -> Prop

-- Self-evident base case (no body)
evident sorted []

-- Conditional case with guard
evident sorted [a, b | rest] when a <= b
    sorted [b | rest]
```

## Types

```evident
-- Built-in
Nat, Int, Real, Bool, String

-- Parameterized
List T
Maybe T
Set T

-- Record
type Task = { id : Nat, name : String, duration : Nat, deadline : Nat }

-- Algebraic
type Color = Red | Green | Blue
type Tree T = Leaf | Node (Tree T) T (Tree T)

-- Constrained type parameter
[T : Ordered]     -- T must be a type with a total ordering
[T : Eq]          -- T must support equality testing
```

## Evidence bodies

```evident
-- Claim head: claim name followed by argument names (no parentheses)
evident claim_name arg1 arg2

-- Body: indented sub-claims that must all be established
    sub_claim1 arg1
    sub_claim2 arg2 arg3
    arg1 = expression    -- arithmetic / equality constraint

-- Guard: when-condition on the same line as the claim head
evident claim_name arg1 arg2 when condition
    ...
```

## Multiple clauses (alternatives)

Multiple `evident` declarations for the same claim name are independent alternatives.
Any one sufficing establishes the claim. They are unordered.

```evident
evident sorted []
evident sorted [_]
evident sorted [a, b | rest] when a <= b
    sorted [b | rest]
```

## Type parameters

```evident
-- Single type parameter
claim member[T : Eq] : T -> List T -> Prop

-- Multiple type parameters
claim zip[A, B] : List A -> List B -> List (A, B) -> Prop

-- Constrained
claim max_of[T : Ordered] : T -> T -> T -> Prop
```

## Queries

```evident
-- Check if a claim holds
? sorted [1, 2, 3]

-- Find a value: ?name is an output variable
? sort [3, 1, 2] ?result

-- Find all: collect all solutions
? all member ?x [1, 2, 3]

-- Find with evidence
? sort [3, 1, 2] ?result with evidence
```

## Assertions (ground facts)

```evident
assert edge "london" "paris"
assert edge "paris" "berlin"
assert task { id = 1, name = "deploy", duration = 60, deadline = 480 }
```

## Forward implication

```evident
-- If card_valid is established, payment_authorized becomes evident
card_valid => payment_authorized

-- Parameterized forward implication
admin_user u => can_access u any_resource
```

## Determinism annotation (optional)

```evident
claim factorial : Nat -> Nat -> det    -- exactly one result
claim member    : Nat -> List Nat -> semidet  -- holds or doesn't
claim path      : Node -> Node -> nondet  -- possibly many paths
```
