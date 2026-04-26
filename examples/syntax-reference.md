# Evident Syntax Reference

Quick reference for the syntax used in the examples. These are working decisions, not final specifications.

Evident accepts both ASCII and Unicode syntax. Every operator has an ASCII form and a Unicode equivalent — they mean exactly the same thing. The Unicode forms are shown in the editor by default; the ASCII forms are what you type.

## Editor shortcuts

Type the ASCII shortcut and the editor replaces it with the Unicode symbol automatically:

| You type | Editor shows |
|---|---|
| `\in` | `∈` |
| `\notin` | `∉` |
| `\->` or `\to` | `→` |
| `\=>` or `\Rightarrow` | `⇒` |
| `\exists` or `\ex` | `∃` |
| `\forall` or `\all` | `∀` |
| `\exists!` | `∃!` |
| `\neg` | `¬` |
| `\and` or `\wedge` | `∧` |
| `\or` or `\vee` | `∨` |
| `\leq` | `≤` |
| `\geq` | `≥` |
| `\neq` | `≠` |
| `\subset` | `⊆` |

## Claims

```evident
-- ASCII form
claim sorted[T : Ordered] : List T -> Prop

-- Self-evident base case (no body)
evident sorted []

-- Conditional case with guard
evident sorted [a, b | rest] when a <= b
    sorted [b | rest]
```
```evident
-- Unicode form (equivalent)
claim sorted[T ∈ Ordered] : List T → Prop

-- Self-evident base case (no body)
evident sorted []

-- Conditional case with guard
evident sorted [a, b | rest] when a ≤ b
    sorted [b | rest]
```

## det claims — binding the result

A `det` claim has exactly one result. Use `=` to bind it:

```evident
-- ASCII
claim sum : Nat -> Nat -> Nat -> det

_total = sum a b        -- bind result to _total
c      = sum 3 4        -- bind result to c
sum a b = 10            -- constrain: the sum must equal 10
```
```evident
-- Unicode
claim sum : Nat → Nat → Nat → det

_total = sum a b
c      = sum 3 4
sum a b = 10
```

`semidet` and `Prop` claims are constraints — they appear without `=`:

```evident
sorted ys               -- holds or doesn't; no result to bind
prime n
permutation xs ys
```

Determinism annotation determines which form is valid:
- `det` → use `= claim args` or `claim args = value`
- `semidet` / `Prop` / `nondet` → use `claim args` as a constraint

## Types

```evident
-- ASCII form

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
```evident
-- Unicode form (equivalent)

-- Built-in
Nat, Int, Real, Bool, String

-- Parameterized
List T
Maybe T
Set T

-- Record
type Task = { id ∈ Nat, name ∈ String, duration ∈ Nat, deadline ∈ Nat }

-- Algebraic
type Color = Red | Green | Blue
type Tree T = Leaf | Node (Tree T) T (Tree T)

-- Constrained type parameter
[T ∈ Ordered]     -- T must be a type with a total ordering
[T ∈ Eq]          -- T must support equality testing
```

## Evidence bodies

```evident
-- ASCII form

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
```evident
-- Unicode form (equivalent)

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

## Body-internal names

Any name appearing in a body but not in the head is **implicitly existential** —
the solver finds a value for it. No `∃` declaration needed.

Use the `_` prefix convention for names that are implementation scaffolding
with no domain meaning:

```evident
-- ASCII
evident product (succ a) b c
    _partial = product a b   -- _partial: body-internal, solver finds it
    c        = sum _partial b
```
```evident
-- Unicode (same)
evident product (succ a) b c
    _partial = product a b
    c        = sum _partial b
```

Names without `_` that appear only in the body are also implicitly existential —
`_` is a convention, not a syntax rule. Use it for intermediate values that have
no meaningful name in the domain.

**Head names** (parameters) are bound from outside.
**Body-only names** are found by the solver.
All body conditions are simultaneous — there is no ordering.

## Multiple clauses (alternatives)

Multiple `evident` declarations for the same claim name are independent alternatives.
Any one sufficing establishes the claim. They are unordered.

```evident
-- ASCII form
evident sorted []
evident sorted [_]
evident sorted [a, b | rest] when a <= b
    sorted [b | rest]
```
```evident
-- Unicode form (equivalent)
evident sorted []
evident sorted [_]
evident sorted [a, b | rest] when a ≤ b
    sorted [b | rest]
```

## Type parameters

```evident
-- ASCII form

-- Single type parameter
claim member[T : Eq] : T -> List T -> Prop

-- Multiple type parameters
claim zip[A, B] : List A -> List B -> List (A, B) -> Prop

-- Constrained
claim max_of[T : Ordered] : T -> T -> T -> Prop
```
```evident
-- Unicode form (equivalent)

-- Single type parameter
claim member[T ∈ Eq] : T → List T → Prop

-- Multiple type parameters
claim zip[A, B] : List A → List B → List (A, B) → Prop

-- Constrained
claim max_of[T ∈ Ordered] : T → T → T → Prop
```

## Queries

```evident
-- ASCII form

-- Check if a claim holds
? sorted [1, 2, 3]

-- Find a value: ?name is an output variable
? sort [3, 1, 2] ?result

-- Find all: collect all solutions
? all member ?x [1, 2, 3]

-- Find with evidence
? sort [3, 1, 2] ?result with evidence
```
```evident
-- Unicode form (equivalent)

-- Check if a claim holds
? sorted [1, 2, 3]

-- Find a value: ?name is an output variable
? sort [3, 1, 2] ?result

-- Find all: collect all solutions
? ∀ member ?x [1, 2, 3]

-- Find with evidence
? sort [3, 1, 2] ?result with evidence
```

## Assertions (ground facts)

```evident
-- ASCII form
assert edge "london" "paris"
assert edge "paris" "berlin"
assert task { id = 1, name = "deploy", duration = 60, deadline = 480 }
```
```evident
-- Unicode form (equivalent)
assert edge "london" "paris"
assert edge "paris" "berlin"
assert task { id = 1, name = "deploy", duration = 60, deadline = 480 }
```

## Forward implication

```evident
-- ASCII form

-- If card_valid is established, payment_authorized becomes evident
card_valid => payment_authorized

-- Parameterized forward implication
admin_user u => can_access u any_resource
```
```evident
-- Unicode form (equivalent)

-- If card_valid is established, payment_authorized becomes evident
card_valid ⇒ payment_authorized

-- Parameterized forward implication
admin_user u ⇒ can_access u any_resource
```

## Quantifiers

```evident
-- ASCII

-- Universal: always explicit
all x in S : P x
all a, b in S : P a b         -- multiple variables, same set

-- Existential in a query: explicit
? some x in S : P x
? some a, b in S : P a b

-- Existential in a body: implicit (just use the name)
evident my_claim a
    b in S                    -- implicitly: there exists b in S
    P a b                     -- b is available here
```
```evident
-- Unicode

-- Universal: always explicit
∀ x ∈ S : P x
∀ a, b ∈ S : P a b

-- Existential in a query: explicit
? ∃ x ∈ S : P x
? ∃ a, b ∈ S : P a b

-- Existential in a body: implicit
evident my_claim a
    b ∈ S
    P a b
```

## Boolean connectives

```evident
-- ASCII form
P and Q
P or Q
not P
a <= b
a >= b
a != b
```
```evident
-- Unicode form (equivalent)
P ∧ Q
P ∨ Q
¬P
a ≤ b
a ≥ b
a ≠ b
```

## Determinism annotation (optional)

```evident
-- ASCII form
claim factorial : Nat -> Nat -> det    -- exactly one result
claim member    : Nat -> List Nat -> semidet  -- holds or doesn't
claim path      : Node -> Node -> nondet  -- possibly many paths
```
```evident
-- Unicode form (equivalent)
claim factorial : Nat → Nat → det    -- exactly one result
claim member    : Nat → List Nat → semidet  -- holds or doesn't
claim path      : Node → Node → nondet  -- possibly many paths
```
