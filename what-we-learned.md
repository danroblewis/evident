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

## The claim body is a flat list of constraints

Everything in a claim is a constraint. Type membership declarations and relational
constraints are the same kind of thing — all conditions that must hold simultaneously.
There is no structural distinction between "the parameter list" and "the body."

```evident
claim distance_between
    g ∈ Graph           -- type constraint: g must be a Graph
    a ∈ g.nodes         -- membership: a must be a node in g
    b ∈ g.nodes         -- membership: b must be a node in g
    d ∈ Nat             -- type constraint: d must be a natural number
    path_between g a b _path    -- relational constraint
    path_length _path d         -- relational constraint
```

The `claim` keyword introduces a name. Everything indented below is a constraint.
Named variables (no `_` prefix) are accessible from outside — they are the claim's
interface. Variables with `_` prefix are internal scaffolding.

The `: Type` annotation at the end of a claim signature is **dropped**. There is no
"return type" because there is no return. All variables are declared as constraints.

---

## Existential vs. universal — single instance vs. universal property

A claim body is implicitly **existential** — the solver finds values for any unbound
variables such that all constraints hold simultaneously. Fixing some variables from
outside is fine; the solver handles the rest.

A claim describes a relationship among **specific values**:

```evident
claim distance_between g ∈ Graph, a ∈ g.nodes, b ∈ g.nodes, d ∈ Nat
    path_between g a b _path
    path_length _path d
```

`distance_between my_graph 1 5 d` — "is there a d such that 5 is reachable from 1
at distance d?" The solver finds d. This is the single-instance use.

A **universal property** — something true for all values — requires a separate claim
using explicit `∀`:

```evident
claim all_pairs_have_distance g ∈ Graph
    ∀ a ∈ g.nodes, ∀ b ∈ g.nodes : ∃ d ∈ Nat : distance_between g a b d
```

These are genuinely different things. A claim definition describes a relationship;
universal statements about all values require explicit `∀`. You cannot collapse them
into one claim.

The naming consequence: `a ∈ g.nodes` in a body means "a is some node in g"
(existential — the solver picks one or the caller fixes one). It does NOT mean
"for all nodes a." If you want the universal, write `∀ a ∈ g.nodes`.

---

## ⚠ Under reconsideration: `det` claims and `= claim args`

The rule that `det` claims use `= claim args` binding form is now in question.
See "Claim composition is variable identification" below. The `=` sign implies
function evaluation and directionality that doesn't match what Evident actually does.

---

## Claim composition is variable identification, not function application

**This is the most important unresolved design question in Evident.**

When you invoke a claim in a body, you are not calling a function. You are
**composing two constraint systems** by **identifying variables** across them.

The current scope has some set of variables with constraints between them.
The sub-claim has its own set of variables with constraints between them.
Invoking the sub-claim in a body means: merge the two systems by making
certain variables shared. The sub-claim's constraints now apply to the
merged system through those shared variables.

This is **relational join**, not function application:
- In a join of R(a, b) and S(b, c), the variable `b` is shared — it is
  the same variable in both relations. No direction. No output. No input.
- In Evident, invoking `path_length path _len` in a body means "identify
  my `path` with `path_length`'s path variable, and my `_len` with its
  length variable." Both constraint systems are now jointly constrained.

### Why `=` is wrong

`_len = length path` implies:
- `length path` is evaluated (direction: right to left)
- The result is assigned to `_len` (one-way)

But there is no evaluation. There is no result. There is no direction.
The variables `_len` and `path` are identified with the variables inside
`length`'s constraint system. The solver finds any satisfying assignment.

### Why `∈` in set comprehension is also wrong (for composition)

`_len ∈ { n | path_length path n }` implies:
- The set `{ n | path_length path n }` is computed
- `_len` is checked for membership in that set

This is still functional/directional. You are not computing a set and
then checking membership. You are posting the constraint `path_length path _len`
and letting the solver find a satisfying assignment.

`∈` remains correct for genuine set membership in data: `a ∈ g.nodes`
is truly a membership check in a collection. But for claim composition it
describes the semantics (what is true) rather than the mechanism (constraint joining).

### The lambda calculus parallel

This resembles lambda calculus substitution: `(λx. body)[x := my_var]`
replaces `x` with `my_var` throughout `body`. But lambda substitution is:
- **Directional**: arguments go in, results come out
- **Ordered**: evaluation follows the expression structure
- **Consuming**: the variable is eliminated

Evident's variable identification is none of these. There is no direction,
no order, and both variables remain — they simply become one.

### All variables are equal

A claim has no distinction between "parameters" (exposed) and "internal"
variables (hidden). All variables in a claim are part of its constraint system.
"Parameterizing" a claim means choosing which of its variables to identify
with variables in the outer scope. The unchosen variables remain free — the
solver finds values for them.

There is no hiding. There is no encapsulation at the variable level.
A claim's named variables are its full interface, and that interface is
everything.

### What the honest notation might be

The Prolog-style positional application is actually **correct** for expressing
constraint joining — not because Prolog is right about execution order, but
because `path_length path _len` with no `=` sign says:

> "Join this constraint system with `path_length`'s constraint system,
> identifying `path` with `path_length`'s path variable and `_len` with
> its length variable."

The problem with Prolog was never the notation for constraint application.
It was the execution model and clause ordering. The notation itself captures
variable identification without implying directionality.

### Open question

We do not yet have a settled syntax for claim composition in a body.
The options on the table:
1. **Positional**: `path_length path _len` — honest, but looks like function call
2. **Named identification**: something like `with path_length: path = my_path, n = _len` — explicit but verbose
3. **Set comprehension**: `_len ∈ { n | path_length path n }` — describes semantics correctly but implies computation

The right answer is unresolved. It may require a new syntactic form that
doesn't exist in any prior language.

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
