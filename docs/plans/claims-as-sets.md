# Claims as sets: set algebra over solution spaces

**Status:** design proposal (2026-06-12). Forward-looking; nothing here is
grammar yet. Motivated by the "make the code more set-theoretic, less
ternary/match" refactor (`docs/plans/ternary-refactor-census.md`).

## The thesis

Evident already declares what it is: *"The central abstraction is `schema`
(or `type` / `claim`): a named set defined by membership conditions."*
A claim is a predicate; a predicate over variables denotes a **set** — its
solution space, `⟦C⟧ = { σ | C(σ) holds }`. So **claims are already sets.**
What's missing is the **algebra**: the ability to intersect, union,
subtract, and *move around* those sets as first-class things instead of
re-deriving them by hand with `match`, `⇒`-chains, and ternaries.

The ask, in one line: let me write

```evident
bday ∈ (ValidBirthdays ∩ DaysAfter2024)
```

and have it mean what it reads — the set of valid birthdays, intersected
with the set of days after 2024, tested for membership of `bday` — instead
of spelling the intersection out as a conjunction of ad-hoc predicates
every time.

## You already have half of it

Two existing mechanisms are set operations wearing other names:

- **`..ClaimName` (lift) is intersection-into-scope.** It conjoins the
  claim's body with the current scope — `..A ..B` constrains the current
  solution space to `here ∩ ⟦A⟧ ∩ ⟦B⟧`. When you write `..DriverInput`,
  you are already intersecting with a set. (You noticed this.)
- **`(a, b) ∈ ClaimName` (positional bind) is membership.** It tests that
  the tuple `(a,b)` is in the claim's solution set over its header slots.
  `bday ∈ ValidBirthdays` (single slot) is the unary case.

So the primitives "intersect" and "is-a-member-of" exist. What's missing:
**union (`∪`), difference (`∖`), relative complement (`∁`)**, set
operations over a **distinguished element** (membership-style, not
whole-scope merge), and the ability to **name and pass a combined set as a
value**.

## What a claim-as-set is: intensional, not extensional

Evident already has `Set(T)` — an *extensional* set (Z3 array-backed,
enumerable membership). This proposal is about *intensional* sets:
predicate-defined, possibly infinite, never enumerated. The two are
complementary cousins:

| | extensional `Set(T)` | intensional claim-as-set |
|---|---|---|
| defined by | the elements it contains | a membership predicate |
| `{2, 4, 6}` | yes | `{ n ∈ Int \| even(n) ∧ 0<n }` |
| infinite? | no | yes |
| representation | array/len | a formula with one free element var |
| "lazy" | n/a | **inherent** — it's just the predicate |

Your instinct — *"push them around as little constraint systems, lazily,
without materializing all values"* — is exactly the intensional model. A
set **is** its defining constraint system. Operations compose the
formulas; membership instantiates the free element; nothing is ever
enumerated. This maps cleanly to SMT: an intensional set is a unary
predicate (`define-fun S ((d Date)) Bool …`), intersection is
`λd. S₁(d) ∧ S₂(d)`, membership is `(S x)`. Z3 has `define-fun` and
lambdas, so the lowering is feasible (with caveats — see below).

## The element-signature discipline (your "same set" problem)

You identified the real constraint: *to intersect two sub-claims they must
be "in the same set."* Precisely — **set operations require a common
element type.** A set of `Date` and a set of `String` cannot be
intersected; the operation is ill-typed, not empty.

So a claim-as-set carries an **element signature** — its header:

```evident
claim ValidBirthdays(d ∈ Date)      -- a set of Date
    1 ≤ d.month ≤ 12
    1 ≤ d.day ≤ daysIn(d.month, d.year)

claim DaysAfter2024(d ∈ Date)       -- also a set of Date
    d.year > 2024
```

`ValidBirthdays ∩ DaysAfter2024` is well-typed because both carry element
`Date`. The element can be a **tuple** — then the claim is a *relation*,
and the algebra is over relations (this is also how your **goal #2**,
"expressions that relate multiple variables mathematically," falls out:
`claim Pythagorean(a, b, c ∈ Int) : a²+b² = c²` is a set of triples, and
you can intersect it with `claim Acute(a,b,c ∈ Int) : c²<a²+b²`).

The algebra, then:

```evident
A ∩ B   ≜  the set of elements in both      (predicate: A(e) ∧ B(e))
A ∪ B   ≜  the set in either                 (A(e) ∨ B(e))
A ∖ B   ≜  in A but not B                     (A(e) ∧ ¬B(e))
∁A      ≜  complement within the element type (¬A(e)) — relative only
e ∈ A   ≜  membership                          (A(e))
A ⊆ B   ≜  containment                         (∀e. A(e) ⇒ B(e))  — a CHECK
A ≡ B   ≜  equal solution sets                 (∀e. A(e) ⇔ B(e))  — model-diff
```

Note `A ≡ B` and `A ⊆ B` are exactly what `scripts/model-diff.sh` computes
today. **model-diff is the comparison half of this algebra; this proposal
is the construction half.** Same foundation (a claim is a solution set),
two directions.

## The payoff for the refactor: partitions become structural

A `match` / ternary chain is a **partition of the input space** with a
behavior per region. Written with named sets, the partition is explicit and
its health is *checkable*:

```evident
-- regions of the input space, named and composable
claim Weekend(d ∈ Day)  : d.name ∈ {Sat, Sun}
claim Holiday(d ∈ Day)  : d ∈ holidayTable
DayOff ≜ Weekend ∪ Holiday          -- a named, reusable region

d ∈ DayOff   ⇒  schedule = Closed
d ∉ DayOff   ⇒  schedule = Open
```

vs. the ternary/boolean soup it replaces. But the real win is that the two
hazards `model-diff` catches become **set identities Z3 can verify
directly**:

- **coverage** (no missing case) ⟺ `⋃ regions ≡ Universe` ⟺
  `¬∃ e : e ∉ any region`
- **disjointness** (no double case) ⟺ `regionᵢ ∩ regionⱼ ≡ ∅`

A first-class `partition` construct could assert both and have the compiler
discharge them:

```evident
partition Day into Weekend | Holiday | Workday   -- Z3 proves total + disjoint
```

This is the "outputs must be COVERED" trap (CLAUDE.md) turned from a
runtime/perf footgun into a *static, named guarantee*. It is also exactly
**goal #1** ("how the solution space is divided up, different behavior per
section") expressed directly.

## Two tiers (ship the cheap one first)

### Tier 1 — membership sugar (pure desugar, fast-path-safe)
When a set expression appears in **membership position**, desugar it to
boolean combinations of the underlying claim memberships:

```
e ∈ (A ∩ B)   ⟿   e ∈ A ∧ e ∈ B
e ∈ (A ∪ B)   ⟿   e ∈ A ∨ e ∈ B
e ∈ (A ∖ B)   ⟿   e ∈ A ∧ ¬(e ∈ B)
∀ e ∈ A : P   ⟿   ∀ e ∈ Carrier : A(e) ⇒ P
∃ e ∈ A : P   ⟿   ∃ e ∈ Carrier : A(e) ∧ P
```

No new semantics, no new runtime — a **pre-oracle transform** in the
passes pipeline (`compiler2/passes/`, alongside the bounded-Seq lowering).
This alone covers most of the refactor: named, composable regions tested
for membership. It stays on the functionizer fast path because it lowers to
the same bool ops you'd write by hand. **This is the high-leverage, low-risk
piece — build it first.**

### Tier 2 — first-class set values (the deep version)
Sets as bindable, passable, returnable values:

```evident
Birthdays2024 ∈ Set⟦Date⟧ = ValidBirthdays ∩ DaysAfter2024
bday ∈ Birthdays2024

claim Filtered(s ∈ Set⟦Date⟧, lo ∈ Date)   -- a claim parameterized by a set
    ...
```

This needs an intensional-set *representation* the language can carry and
pass (a predicate value / `define-fun` / lambda). It's the genuinely new
capability. Caveats below. Design it; gate it; don't block Tier 1 on it.

## Hard problems / open questions

1. **Signature alignment for relations.** Unary sets are easy. For tuple
   carriers, `A ∩ B` must align signatures — by position? by header name?
   What if `A(a,b)` and `B(b,a)`? Probably: align by header name, require
   matching element types, allow an explicit reorder map (like composition's
   `slot ↦ value`). Needs a ruling.
2. **Complement needs a universe.** `∁A` is only meaningful relative to the
   element type's domain (`∁A = Carrier ∖ A`). Fine for bounded carriers;
   for `Int` it's the (infinite) intensional complement `¬A(e)` — sound but
   only useful inside another bounded context.
3. **First-class set values vs the functionizer.** An intensional set is a
   predicate, not a scalar. A set-*valued carry* or higher-order set
   argument will likely go **residual** (the functionizer extracts scalar
   assignments, not predicates). Rule of thumb: Tier 1 (desugared
   membership) stays fast; Tier 2 first-class values are for **expressivity
   in cold code**, not hot carried state. Measure before using one on the
   compiler's hot path.
4. **Set-typed state.** Can an `fsm` carry a `Set⟦T⟧`? Semantically yes (a
   carried predicate), but see (3) — probably not on a hot tick loop yet.
5. **Operator surface.** `∩ ∪ ∖ ∁ ⊆ ≡` as expression operators in
   membership/binding position is **new grammar** — it needs an operator
   ruling (the critic flags new constructions; it never approves them). The
   `≜` set-definition binder, the `partition … into …` construct, and
   `Set⟦T⟧` (intensional, distinct from extensional `Set(T)`) are all new
   surface to be ruled on.
6. **Naming collision with `Set(T)`.** Extensional `Set(T)` already exists.
   Intensional sets need a distinct spelling (`Set⟦T⟧`? `Claim⟦T⟧`? a
   keyword?) so the two models don't blur.
7. **Empty/universe literals.** `∅⟦T⟧` and `Universe⟦T⟧` (or `⊤⟦T⟧`) as the
   identity elements of `∪`/`∩`, useful for partition checks.

## Relation to existing constructs (so this composes, not bolts on)

- `type` / `claim` — already "named set defined by membership conditions."
  This makes that explicit and compositional.
- `..ClaimName` — already intersection-into-scope (whole-scope, not
  element-keyed). The membership form is the element-keyed sibling.
- `(a,b) ∈ ClaimName`, bare-mention, headers — already membership / join on
  header slots. The algebra adds `∪`/`∖` and set-as-value.
- `Set(T)` — the extensional cousin; keep both, name them apart.
- **Lineage:** this is refinement / predicate-subtype theory (PVS predicate
  subtypes, Liquid types, F* refinements, set-builder notation). `claim
  ValidBirthdays(d ∈ Date) <pred>` *is* the refinement type
  `{ d : Date | pred }`, and intersection of refinements is conjunction of
  predicates. Well-trodden ground; Evident is unusually well-positioned for
  it because its claims are *already* solution sets and Z3 is already in the
  loop.

## Implementation path (Evident-native; no Rust)

1. **Tier 1 desugar** as a pre-oracle pass (`compiler2/passes/`): recognize
   set expressions (`A ∩ B`, `A ∪ B`, `A ∖ B`) in membership/quantifier
   position and rewrite to the bool combinations above. Gate it with
   `model-diff` — a set-algebra rewrite is correct iff `model-diff` reports
   the rewritten claim `≡` the hand-written one. (This pass is itself a nice
   first citizen of the "passes-seam" work the self-host roadmap needs.)
2. **`partition` construct** + coverage/disjointness discharge — emits the
   two set-identity checks to Z3. High value, moderate effort, builds on
   Tier 1.
3. **Tier 2 first-class set values** — needs the intensional-set
   representation and a `Set⟦T⟧` type. Design as a follow-on; honestly gated
   on the functionizer story (3) for any hot use.

## Why now / why this matters

The refactor you want (set-theoretic over ternary) is **starved for sets**
— you have types and `Set(T)` but no way to name and compose *regions of a
solution space*. This proposal gives you that vocabulary, and it does so by
completing a model Evident already committed to ("schemas are sets") rather
than importing a foreign one. The cheap tier (membership desugar) is mostly
syntax over semantics you already have, ships as one pass, and is verified
by a tool that already exists (`model-diff`). The expensive tier (set
values) is real new capability with a real perf caveat, to be designed
deliberately. Either way the win is the same: **the structure of the
solution space — its regions, their coverage, their overlap — becomes
something you write down and the compiler checks, instead of something you
encode by hand in match arms and ternary spines and hope you got total.**
