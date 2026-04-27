# Evident Specification — Types

## Overview

In Evident, a type IS a set. `Nat` names the set of all natural numbers.
`Task` names the set of all records satisfying the Task field conditions. The `type`
keyword is syntax sugar for a claim whose body consists entirely of membership
conditions — it has no special status in the runtime.

Types obey the same set-theoretic semantics as everything else:
- `x ∈ Task` means x is a member of the Task set
- `type Task = { ... }` declares what membership in Task requires
- Constraints inside a type body narrow the set; the solver cannot produce a value of that type that violates them

---

## Record types

A record type is a named set of records. Each field line is a membership condition:
the field value must belong to the named set.

```evident
type Task = {
    id       ∈ Nat
    name     ∈ String
    duration ∈ Nat
}
```

`Task` is the set of all records that have an `id` in `Nat`, a `name` in `String`,
and a `duration` in `Nat`. A record literal satisfies this type if and only if each
field value is a member of the corresponding set.

Fields are accessed with `.`:

```evident
task.id       -- the id field
task.duration -- the duration field
```

Inline record syntax for assertions and queries:

```evident
assert task { id = 1, name = "deploy", duration = 60 }

? valid_task { id = 2, name = "review", duration = 30 }
```

A more complete example with several types:

```evident
type Worker = {
    id              ∈ Nat
    name            ∈ String
    available_from  ∈ Nat   -- minutes from start of day
    available_until ∈ Nat
}

type Assignment = {
    task_id   ∈ Nat
    worker_id ∈ Nat
    start     ∈ Nat
}
```

Fields listed on separate indented lines. The record body is a layout block —
the same indentation rule as claim bodies.

Compact inline form (equivalent):

```evident
type Point = { x ∈ Int, y ∈ Int }
```

---

## Algebraic / sum types

Sum types enumerate the possible forms a value can take. Each variant is a
constructor name (uppercase). Variants may carry data.

```evident
type Color = Red | Green | Blue
```

`Color` is the set `{ Red, Green, Blue }`. A value is a member of `Color` if and
only if it is one of those constructors.

Variants with data use positional arguments in parentheses:

```evident
type Tree T = Leaf | Node (Tree T) T (Tree T)
```

`Leaf` is a tree. `Node left value right` is a tree when `left` and `right` are
trees of the same element type and `value` is an element. `Tree T` is the least
fixed point of this equation — the smallest set satisfying it.

Another example — a type for optional values:

```evident
type Maybe T = Nothing | Just T
```

`Maybe Nat` is the set containing `Nothing` and all values of the form `Just n`
where `n ∈ Nat`.

Pattern matching on sum type variants in claim bodies uses structural evident clauses:

```evident
claim tree_size[T]
    t    ∈ Tree T
    size ∈ Nat

evident tree_size Leaf 0

evident tree_size (Node left _ right) size
    tree_size left  left_size
    tree_size right right_size
    size = left_size + right_size + 1
```

---

## Refinement types

A record type can carry arbitrary constraints alongside its field membership
conditions. These constraints narrow the set — they are additional membership
requirements beyond field types alone.

```evident
type ValidAssignment = {
    talk ∈ Talk
    room ∈ Room
    slot ∈ Slot
    talk.duration ≤ slot.end - slot.start
    room.capacity ≥ talk.expected_audience
}
```

`ValidAssignment` is not just any record with a talk, room, and slot — it is the
subset of such records where the talk fits within the slot's time window and the
room can hold the audience. The solver cannot produce a `ValidAssignment` that
violates these constraints.

A constraint line inside a type body is identical in meaning to a constraint line
inside a claim body. Fields and constraints are both membership conditions; the
`type` form simply groups them.

Example — a bounded natural number:

```evident
type BoundedNat N = {
    value ∈ Nat
    value < N
}
```

`BoundedNat 100` is the set of natural numbers less than 100.

Example — a non-empty list:

```evident
type NonEmptyList T = {
    items ∈ List T
    |items| ≥ 1
}
```

Example — a time window with a positive span:

```evident
type Window = {
    start ∈ Nat
    end   ∈ Nat
    start < end
}
```

---

## Generic type parameters

Type parameters are variables in the constraint system. They appear after the type
name, before `=`. They are constrained with `∈` like any other variable.

```evident
type Pair A B = {
    first  ∈ A
    second ∈ B
}
```

`Pair Nat String` is the set of records where `first ∈ Nat` and `second ∈ String`.

Constrained type parameters narrow what types may be substituted:

```evident
type OrderedList T, T ∈ Ordered = {
    items ∈ List T
    ∀ (a, b) ∈ each_consecutive items : a ≤ b
}
```

`T ∈ Ordered` is a constraint on the type variable `T` itself — `T` must be a
member of the set `Ordered`, meaning it must be a type that supports a total
ordering. You cannot form `OrderedList Color` because `Color` is not in `Ordered`.

Bracket notation `[T ∈ Ordered]` is accepted as shorthand and is equivalent:

```evident
type OrderedList[T ∈ Ordered] = {
    items ∈ List T
    ∀ (a, b) ∈ each_consecutive items : a ≤ b
}
```

Multiple parameters:

```evident
type RelationBetween A B = {
    pairs ⊆ A × B
}
```

---

## Pass-through type composition

`..OtherType` inside a type body lifts all variables and constraints from
`OtherType` into the current type. This is the type-level equivalent of the
pass-through operator for claims.

```evident
type OrderedArray[T ∈ Ordered] = {
    ..Indexable T
    ∀ (a, b) ∈ consecutive_pairs : a ≤ b
}
```

`Indexable T` defines fields `n`, `entries`, and `consecutive_pairs`. The `..`
operator imports all of them into `OrderedArray T`, including the `Indexable T`
constraints. The `∀` line then adds one more condition: consecutive pairs must be
non-decreasing. Every value of type `OrderedArray T` is also a value of type
`Indexable T`.

Pass-through composes types without inheritance. There is no subtype relationship
here — `OrderedArray T` and `Indexable T` are sets, and `OrderedArray T ⊆ Indexable T`
holds as a consequence of the membership conditions.

Another example — extending a base record type:

```evident
type TimedTask = {
    ..Task
    deadline ∈ Nat
    deadline ≥ duration    -- deadline must be at least as long as the task
}
```

`TimedTask` has all fields of `Task` plus a `deadline` field, plus the additional
constraint that the deadline is at least as large as the duration.

---

## Built-in types

The following types are provided by the runtime and require no definition.

| Type | Description | Notes |
|---|---|---|
| `Nat` | Natural numbers: 0, 1, 2, ... | No upper bound |
| `Int` | Integers: ..., -2, -1, 0, 1, 2, ... | |
| `Real` | Real numbers | Solver uses rational arithmetic for decidability |
| `Bool` | `{ true, false }` | |
| `String` | Text sequences | Equality and ordering defined |
| `List T` | Ordered sequence of elements of type T | Defined as a set of (Nat, T) index-value pairs |
| `Set T` | Unordered collection of T with no duplicates | |
| `(A, B)` | Ordered pair of A and B | Generalises to `(A, B, C, ...)` for n-tuples |
| `A × B` | Cartesian product — the type of sets of (A, B) pairs | Used as the type of relations |

Numeric relationships among built-in types:

```
Nat ⊆ Int ⊆ Real
```

A natural number is also an integer. An integer is also a real. The solver
respects these inclusions; `n ∈ Nat` implies `n ∈ Int`.

`List T` as a set of index-value pairs means `each_consecutive`, `first_of`, and
similar list operations are definable in terms of membership in that set:

```evident
-- consecutive_pairs for a list: pairs at adjacent indices
consecutive_pairs list = { (v1, v2) | (i, v1) ∈ list, (i+1, v2) ∈ list }
```

---

## Type constraints (type-class-like sets)

`Ordered` and `Eq` are sets whose members are types. They are constraints on type
variables, not a separate mechanism — they use the same `∈` operator as everything
else.

```evident
T ∈ Ordered    -- T is a type with a total ≤ ordering
T ∈ Eq         -- T is a type that supports equality testing
```

Built-in memberships:

| Type | In `Eq`? | In `Ordered`? |
|---|---|---|
| `Nat` | yes | yes |
| `Int` | yes | yes |
| `Real` | yes | yes |
| `Bool` | yes | yes |
| `String` | yes | yes (lexicographic) |
| `List T` (T ∈ Eq) | yes | if T ∈ Ordered |
| `(A, B)` (A, B ∈ Eq) | yes | if A, B ∈ Ordered (lexicographic) |

User-defined types are in `Eq` if their fields are all in `Eq`. They are in
`Ordered` only if an ordering claim is explicitly provided.

`Ordered` and `Eq` are used as constraints on type parameters in claim and type
declarations:

```evident
claim max_of[T ∈ Ordered]
    a   ∈ T
    b   ∈ T
    m   ∈ T
    m = a ∨ m = b
    m ≥ a
    m ≥ b

claim member[T ∈ Eq]
    x    ∈ T
    list ∈ List T
    ∃ (_, v) ∈ list : v = x
```

---

## Recursive types

Recursive types are well-defined as the least fixed point of their defining
equations. The solver treats recursive type definitions as the smallest set
satisfying the equations.

```evident
type Tree T = Leaf | Node (Tree T) T (Tree T)

type NatList = Nil | Cons Nat NatList
```

These are standard coinductive/inductive definitions. The language makes no
syntactic distinction between them; the solver handles finite inductive structures.

---

## Type aliases

A `type` declaration with no field constraints is a simple alias:

```evident
type Schedule = List Assignment
type NodeId   = Nat
type Matrix   = List (List Real)
```

`Schedule` and `List Assignment` are the same set — the alias only introduces
a shorter name. No new type is created; membership in `Schedule` is identical to
membership in `List Assignment`.

---

## Desugaring: `type` as a `claim`

`type` is syntax sugar. The following are equivalent:

```evident
-- Sugared form
type Task = {
    id       ∈ Nat
    name     ∈ String
    duration ∈ Nat
}
```

```evident
-- Desugared: a claim that defines Task as a set
claim Task
    id       ∈ Nat
    name     ∈ String
    duration ∈ Nat
```

The desugared form makes clear that `Task` is a set defined by membership
conditions, identical in structure to any other claim. The `type` keyword signals
to the reader that this claim's purpose is to name a type, not to encode domain
logic — it is a readability convention, not a semantic distinction.

---

## Complete example: conference scheduling types

```evident
type Talk = {
    id               ∈ Nat
    title            ∈ String
    speaker          ∈ String
    duration         ∈ Nat       -- minutes
    expected_audience ∈ Nat
}

type Room = {
    id       ∈ Nat
    name     ∈ String
    capacity ∈ Nat
}

type Slot = {
    id    ∈ Nat
    start ∈ Nat    -- minutes from start of day
    end   ∈ Nat
    start < end    -- refinement: slot must have positive duration
}

type ValidAssignment = {
    talk ∈ Talk
    room ∈ Room
    slot ∈ Slot
    talk.duration ≤ slot.end - slot.start
    room.capacity ≥ talk.expected_audience
}

type ConferenceSchedule = {
    assignments ∈ Set ValidAssignment
    -- every talk appears at most once
    ∀ a, b ∈ assignments : a.talk.id = b.talk.id ⇒ a = b
    -- no room is double-booked
    ∀ a, b ∈ assignments :
        a ≠ b ∧ a.room.id = b.room.id ⇒
        a.slot.end ≤ b.slot.start ∨ b.slot.end ≤ a.slot.start
}
```

`ValidAssignment` is a refinement type — it is not just any record with a talk,
room, and slot, but only those where the room and slot are compatible with the
talk's requirements. `ConferenceSchedule` further refines the set by requiring that
the schedule as a whole is consistent.

---

## Open questions

- **Inline constraint syntax**: whether field constraints like `start < end` in a
  record body should use `where` to visually separate them from field declarations,
  or whether the flat layout (fields and constraints interleaved) is preferred.

- **Type parameter syntax**: `T ∈ Ordered` (pure relational style) vs `[T ∈ Ordered]`
  (bracket notation as in claims). Both are currently accepted. A future version
  may standardise on one form.

- **Nominal vs structural**: whether two separately-defined record types with
  identical fields are the same type (structural) or different types (nominal).
  Current intent is structural — a type is its membership conditions, and if two
  types have the same conditions they are the same set. This matches set-theoretic
  semantics but may be surprising to users expecting nominal typing.

- **Ordering on user-defined types**: the mechanism for declaring that a
  user-defined type belongs to `Ordered` is not yet specified. One option is a
  dedicated claim; another is an `instance` or `witness` declaration.

- **Infinite / coinductive types**: `List T` and `Tree T` are defined inductively
  (finite structures). Whether Evident supports coinductive types (potentially
  infinite streams) and how they interact with the solver is an open question.

- **`..` and field conflicts**: if `..OtherType` lifts a field named `x` and the
  current type also declares `x`, whether this is an error or a merge (intersecting
  the two membership conditions for `x`) is not yet decided.
