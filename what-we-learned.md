# What We've Learned: Evident Design Notes

A running record of design decisions made through conversation. Each section
records what changed, why, and what the current position is.

---

## Claims define sets

The fundamental insight: every `claim` names a set. Every `evident` block adds a
membership condition, intersecting the set with a smaller one. The programmer's
job is to write claims whose solution space is exactly the collection of interesting
things.

```evident
claim sorted[T ∈ Ordered] : List T → Prop
```

`sorted` names the set of non-decreasing lists of type T. `sorted [1, 2, 3]` is a
membership claim: `[1, 2, 3] ∈ sorted`.

```evident
evident sorted [a, b | rest] when a ≤ b
    sorted [b | rest]
```

This adds a membership condition: `[a, b, ...rest] ∈ sorted` when `a ≤ b` and
`[b, ...rest] ∈ sorted`.

Constraint accumulation = progressive set intersection. Each step narrows the
solution space. The solver finds members of the final intersection.

---

## Type definitions are set-builder notation

`{ id ∈ Nat, duration ∈ Nat }` is set-builder notation. A record type is the set of
all records satisfying the field membership conditions. The colon in type annotations
and `∈` are the same judgment — both mean "is a member of."

```evident
type Task = { id ∈ Nat, duration ∈ Nat, deadline ∈ Nat }
-- same as: { t | t.id ∈ Nat ∧ t.duration ∈ Nat ∧ t.deadline ∈ Nat }
```

---

## `det` claims use `= claim args` form

A `det` claim has exactly one result for any given inputs — it is a function.
Its syntax reflects this: use `=` to bind the result rather than adding a
positional output argument.

```evident
-- Declaration
claim sum : Nat → Nat → Nat → det

-- Calling (binding the result)
_total = sum a b

-- Constraining (the result must equal something)
sum a b = 10
```

`semidet` and `Prop` claims are constraints — they hold or they don't.
They appear in the body without `=`:

```evident
sorted ys       -- semidet: either ys is sorted or it isn't
prime n         -- semidet: n is prime or it isn't
permutation xs ys  -- Prop: xs and ys are permutations of each other
```

The determinism annotation (`det`, `semidet`, `nondet`) on a claim declaration
determines which syntactic form is valid at call sites.

---

## Body statements are simultaneous constraints — no ordering

All statements in a body block are simultaneously true. There is no sequential
introduction of variables. The solver satisfies all conditions at once, in whatever
order it finds efficient.

```evident
-- These two bodies are identical in meaning:
evident my_claim a b
    sum a b = 10
    prime a

evident my_claim a b
    prime a
    sum a b = 10
```

There is no "first compute the sum, then check primality." Both conditions
constrain the same solution space simultaneously.

---

## Body-internal names are implicitly existential

Any name in a body that is not a parameter in the head is implicitly existentially
quantified — the solver finds a value for it. No `∃` declaration needed.

```evident
-- Head parameters: a, b, c (bound from outside)
evident my_claim a b c
    _intermediate = product a b    -- _intermediate: body-internal, solver finds it
    c = sum _intermediate 1
```

The `_` prefix is a convention for names that are implementation scaffolding —
intermediate values with no domain meaning. Names without `_` that appear only
in the body are also implicitly existential; the underscore is just a readability
signal.

The rule: **head names are bound from outside; body-only names are found by the solver.**

---

## `∀` stays explicit; existentials in bodies are implicit

Universal quantification must be stated explicitly because it means something
specific: "for every element of this set, the following must hold."

```evident
-- ∀ is explicit — applies to all elements
∀ t ∈ tasks : deadline_met t schedule

-- Existential is implicit — solver finds a value
_worker ∈ workers
_worker.id = a.worker_id
```

A bare name in a body is existential. `∀ x ∈ S : P x` is universal. These are
genuinely different and the asymmetry is intentional.

---

## `member` is just `∈`

The claim `member x list` is `x ∈ list`. The symbol is already in the language.
No claim needed, no recursive definition needed. Anywhere `member` appeared, use `∈`.

---

## Shorthand for multiple existentials

When several names come from the same set, list them together:

```evident
∃ a, b, c ∈ Nat      -- equivalent to ∃ a ∈ Nat, ∃ b ∈ Nat, ∃ c ∈ Nat
∀ a, b ∈ workers     -- for all pairs of workers
```

---

## Claim names are noun phrases, never action verbs

A claim name answers "what is true?" not "what should be done?"

| Wrong | Right |
|---|---|
| `find_worker id workers` | `_w ∈ workers, _w.id = id` (inline — no claim needed) |
| `remove_one x list result` | Redefine using `occurrences` |
| `sort xs ys` | `SortedOf xs` (type) or `sorted_permutation_of xs ys` |
| `compute_gcd a b` | `gcd a b` (noun: the GCD of a and b) |

The test: is this a noun phrase describing a relationship between things? If it
sounds like a verb (find, get, compute, check, validate, remove), rename it.

---

## No nested `det` calls — flatten with `_` names

`det` results cannot be nested in call position. Bind each intermediate result:

```evident
-- Wrong (nested, functional style):
c = sum (product a b) 1

-- Right (flat, each step named):
_ab = product a b
c   = sum _ab 1
```

This keeps body statements as simultaneous constraints rather than a sequential
evaluation order. `_ab` and `c` are both found by the solver at once; the naming
is for human readability, not for sequencing.

---

## The solution space is geometric

Every claim has a "shape" — the geometry of its solution space in the product of
its argument types.

| Claim | Shape |
|---|---|
| `∃ n ∈ Nat` | 1D: all of Nat |
| `sum a b c` (all free) | 2D surface in Nat³ |
| `sum a b = 10` | 1D line: pairs summing to 10 |
| `c = sum 3 4` | 0D: the single point c=7 |
| `divides a b` | Sparse subset of Nat² |
| `prime n` | Subset of Nat, density ~1/ln(n) |
| `coprime a b` | ~60.8% of Nat² |

The solver navigates this space. Monte Carlo sampling estimates its density and
shape without running any algorithm — just by checking the claim against random
inputs.

---

## Extraction via `∃` witness binding

To use the result of a claim in subsequent body lines, the `∃` introduces a
witness name. For `det` claims, use `= claim args`. For set membership, use `∈`:

```evident
-- det claim: bind with =
_sorted = sort_of xs      -- if sort_of were a det function

-- Set membership: bind with ∈
∃ result ∈ SortedOf xs    -- result is a witness; available below

-- Subsequent lines can use the bound name
first_element result _min
```

For `SortedOf`-style dependent types, the `∃` witness is how you "extract" the
computed value from the solver. This replaces the function-call return value.

---

## ASCII and Unicode are aliases

Every operator has both forms; the editor auto-replaces ASCII to Unicode:

| ASCII | Unicode |
|---|---|
| `x : T` or `x in T` | `x ∈ T` |
| `->` | `→` |
| `=>` | `⇒` |
| `some x in S :` | `∃ x ∈ S :` |
| `all x in S :` | `∀ x ∈ S :` |
| `<=`, `>=`, `!=` | `≤`, `≥`, `≠` |

Type `\in`, `\->`, `\exists`, `\forall`, `\leq`, etc. and the editor inserts
the Unicode form.
