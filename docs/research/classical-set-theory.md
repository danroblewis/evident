# Classical Set Theory Operations: A Reference for Evident's Standard Library

This document surveys the core operations of classical set theory with an eye toward
their computational meaning and their relevance to Evident, a constraint programming
language where programs are collections of constraints over sets.

For each operation we cover: what it is mathematically, what it means computationally,
its type signature, typical notation, and how it fits into constraint/logic programming.

---

## 1. Membership: x ∈ S, x ∉ S

**Mathematical meaning.**
The most primitive relation in set theory. `x ∈ S` asserts that element `x` is a member
of set `S`. `x ∉ S` is its negation.

**Computational meaning.**
At runtime, this is a decision procedure: given a concrete element and a concrete set,
return true or false. For finite sets it is enumeration (or hash lookup); for sets
defined by a predicate it is predicate application.

In constraint programming it is more interesting: if `x` is a free variable, `x ∈ S`
restricts `x`'s domain to `S`. This is the basic domain constraint that drives propagation.
Asserting `x ∈ S` does not pick a value for `x`; it eliminates candidates outside `S`.

**Type signature.** `(α, Set α) → Bool` (test form) or `(α, Set α) → Prop` (constraint form)

**Returns.** Boolean for ground tests; a constraint that shrinks a variable's domain.

**Notation.** `x ∈ S` / `x ∉ S` (Unicode); `x in S` / `x not in S` (ASCII)

**Common in constraint/logic programming?** Yes — this is the foundational operation.
Every Datalog fact is a membership assertion (`edge(a,b)` asserts `(a,b) ∈ edge`).
Every type annotation is membership in a type-as-set.

**Natural inverse / bidirectional use.** Yes. The same constraint `x ∈ S` can be read
forward (test: is x in S?) or backward (generate: what values of x are in S?). In
Evident, `∃ x ∈ S : P(x)` uses it backward — the solver must find a witness.

---

## 2. Subset / Superset: S ⊆ T, S ⊇ T, S ⊂ T

**Mathematical meaning.**
`S ⊆ T` holds when every element of `S` is also an element of `T` (subset or equal).
`S ⊂ T` (strict subset) holds when additionally `S ≠ T`. `S ⊇ T` is just `T ⊆ S`.

**Computational meaning.**
Checking `S ⊆ T` requires verifying `∀ x ∈ S : x ∈ T`. For finite sets this is
O(|S| · lookup-cost). As a constraint, `S ⊆ T` restricts possible values of S to
sets that fit inside T — useful for set-variable problems (e.g., "the solution set
must be a subset of the candidate set").

**Type signature.** `(Set α, Set α) → Bool`

**Returns.** Boolean for ground tests; constraint for set-variable problems.

**Notation.** `S ⊆ T` / `S ⊇ T` / `S ⊂ T`; `S subset of T` (ASCII)

**Common in constraint/logic programming?** Moderately. Directly useful in type
system reasoning (subtyping) and in set-variable CP where variables range over sets.
Less common in pure Datalog.

**Natural inverse.** `S ⊆ T` ↔ `T ⊇ S`. Bidirectional if either set is a variable.

---

## 3. Set Equality: S = T

**Mathematical meaning.**
Two sets are equal if and only if they have exactly the same elements:
`S = T` ↔ `(S ⊆ T) ∧ (T ⊆ S)`. This is the axiom of extensionality.

**Computational meaning.**
For finite sets: sort and compare, or compute symmetric difference and test emptiness.
As a constraint: forces two set expressions to have identical membership, which can
propagate in both directions.

**Type signature.** `(Set α, Set α) → Bool`

**Returns.** Boolean or equality constraint.

**Notation.** `S = T`

**Common in constraint/logic programming?** Yes, especially for asserting that a
derived set equals an expected set (e.g., the set of assigned workers equals the set
of available workers). In Evident, claim equality collapses to this.

**Natural inverse.** Symmetric. `S = T` ↔ `T = S`.

---

## 4. Union: S ∪ T

**Mathematical meaning.**
The set of all elements belonging to S, to T, or to both:
`S ∪ T = { x | x ∈ S ∨ x ∈ T }`.

**Computational meaning.**
For finite sets: merge and deduplicate. Cost O(|S| + |T|) with hashing. As a
constraint, `x ∈ S ∪ T` is equivalent to `x ∈ S ∨ x ∈ T`, which branches the
constraint search.

**Type signature.** `(Set α, Set α) → Set α`

**Returns.** A new set containing all elements of both inputs.

**Notation.** `S ∪ T`; `union(S, T)` (functional)

**Common in constraint/logic programming?** Yes. In Datalog, union corresponds to
having multiple rules for the same predicate (or-composition). In constraint solving,
union domains are disjunctive and harder than intersection domains.

**Natural inverse.** No clean inverse in general. Decomposition: if you know `S ∪ T`
and one of `S`, `T`, you can derive bounds on the other. Full inversion is ambiguous
(many pairs produce the same union).

---

## 5. Intersection: S ∩ T

**Mathematical meaning.**
The set of elements belonging to both S and T:
`S ∩ T = { x | x ∈ S ∧ x ∈ T }`.

**Computational meaning.**
For finite sets: filter one set against the other. Cost O(min(|S|, |T|)) with hashing.
As a constraint, intersection tightens domains: `x ∈ S ∩ T` means `x ∈ S` AND `x ∈ T`,
which is pure conjunction — each constraint propagates independently. Intersection is
therefore cheaper and more propagation-friendly than union.

**Type signature.** `(Set α, Set α) → Set α`

**Returns.** A new set containing only shared elements.

**Notation.** `S ∩ T`; `intersection(S, T)` (functional)

**Common in constraint/logic programming?** Very common. Domain intersection is the
basic mechanism by which constraint propagators narrow variable domains. Every time
two constraints on the same variable are combined, their domains intersect.

**Natural inverse.** No clean inverse. But `S ∩ T = ∅` (disjointness) has the
clean reading: the two sets share nothing. This is itself a useful constraint.

---

## 6. Difference: S \ T

**Mathematical meaning.**
The set of elements in S but not in T:
`S \ T = { x | x ∈ S ∧ x ∉ T }`. Also written `S - T`.

**Computational meaning.**
Filter S to exclude elements appearing in T. Cost O(|S| · lookup-cost-in-T). As a
constraint, `x ∈ S \ T` means `x ∈ S` and `x ∉ T` simultaneously — useful for
exclusion constraints.

**Type signature.** `(Set α, Set α) → Set α`

**Returns.** S with T's elements removed.

**Notation.** `S \ T` or `S - T`; `difference(S, T)` (functional)

**Common in constraint/logic programming?** Moderately. Useful for "S except for
known-bad elements T". In Datalog it requires stratified negation. In Evident, this
would appear as a set comprehension with negated membership.

**Natural inverse.** Not clean. `(S \ T) ∪ T = S ∪ T`, not S — information about
the overlap is lost.

---

## 7. Symmetric Difference: S △ T

**Mathematical meaning.**
Elements in exactly one of S or T, but not both:
`S △ T = (S \ T) ∪ (T \ S) = (S ∪ T) \ (S ∩ T)`.

**Computational meaning.**
Identifies elements that disagree between the two sets. Useful for computing what
changed between two versions of a set (delta), or for asserting that two sets are
"completely different" (disjoint) or "completely the same" (empty symmetric difference).

**Type signature.** `(Set α, Set α) → Set α`

**Returns.** Set of elements in one but not both.

**Notation.** `S △ T` or `S ⊕ T`; `symmetric_difference(S, T)` (functional)

**Common in constraint/logic programming?** Less common than the basic set operations.
Most useful as a derived notion: `S △ T = ∅` is equivalent to `S = T`, which is
sometimes a cleaner way to assert equality via difference. Used in change detection.

**Natural inverse.** `S △ T = U` means `T = S △ U` — symmetric difference is its
own inverse (self-inverse). This is a useful algebraic property.

---

## 8. Complement: Sᶜ

**Mathematical meaning.**
The set of all elements (in some universe U) not in S:
`Sᶜ = U \ S = { x ∈ U | x ∉ S }`.
Always relative to a universe — the "absolute complement" does not exist in ZFC set theory.

**Computational meaning.**
Complement depends on knowing the universe. For a closed finite domain (e.g., the set
of all workers, the set of all tasks) it is straightforward: enumerate the universe and
exclude S. For open or infinite domains, complement requires negation-as-failure or
explicit domain bounds.

**Type signature.** `(Set α, Universe α) → Set α` (where Universe is the containing set)

**Returns.** All universe elements outside S.

**Notation.** `Sᶜ`, `S̄`, `∁S`, `complement(S, U)` (functional)

**Common in constraint/logic programming?** Important but tricky. Complement corresponds
to negation — `x ∈ Sᶜ` is `x ∉ S`. In Datalog, complement requires stratification
because it introduces non-monotonicity (adding elements to S can remove elements from
Sᶜ). In Evident, `none x in S : P(x)` is related to complement.

**Natural inverse.** `(Sᶜ)ᶜ = S` — complement is self-inverse. De Morgan's laws apply.

---

## 9. Cartesian Product: S × T

**Mathematical meaning.**
The set of all ordered pairs from S and T:
`S × T = { (s, t) | s ∈ S ∧ t ∈ T }`.
Cardinality: `|S × T| = |S| · |T|`.

**Computational meaning.**
Generates all pairings. The foundation of relational algebra: a relation is a subset
of a Cartesian product. Computing `S × T` explicitly is expensive (quadratic space),
but constraint systems work lazily — they enumerate pairs only as needed.

In constraint solving, `(x, y) ∈ S × T` decomposes cleanly into `x ∈ S ∧ y ∈ T`,
which is two independent domain constraints. This is the key reason pairs (tuples) are
natural in constraint systems.

**Type signature.** `(Set α, Set β) → Set (α × β)`

**Returns.** Set of all pairs (a, b) with a from S and b from T.

**Notation.** `S × T`; `product(S, T)` (functional)

**Common in constraint/logic programming?** Very. Relations in Datalog/Evident are
subsets of Cartesian products. Joins are intersections over products. The product
structure is everywhere, usually implicit.

**Natural inverse.** Projection (operation 22) extracts components. `π₁(S × T) = S`
and `π₂(S × T) = T` if both are non-empty.

---

## 10. Power Set: 𝒫(S)

**Mathematical meaning.**
The set of all subsets of S:
`𝒫(S) = { T | T ⊆ S }`.
Cardinality: `|𝒫(S)| = 2^|S|`.

**Computational meaning.**
Explicitly constructing the power set is exponential in |S| — impractical for large sets.
In constraint programming, power sets appear implicitly: when a variable ranges over
subsets of a domain, its domain is effectively the power set of the base domain. This
is the realm of set-variable CP (e.g., Gecode's set variables), where propagators
reason over lower and upper bounds on the set variable without enumerating all subsets.

**Type signature.** `Set α → Set (Set α)`

**Returns.** The collection of all subsets.

**Notation.** `𝒫(S)` or `2^S`; `powerset(S)` (functional)

**Common in constraint/logic programming?** Rarely used explicitly due to exponential
cost. The implicit version (set-variable reasoning) is common in set CP. In Evident,
`𝒫(S)` would appear when you need to reason about which subset of a collection
satisfies a property.

**Natural inverse.** Given `T ∈ 𝒫(S)`, you know `T ⊆ S`. The inverse question — "which
set has this as its power set?" — has a unique answer if the power set is given.

---

## 11. Cardinality: |S|

**Mathematical meaning.**
The number of elements in a set. For finite sets this is a natural number. For infinite
sets it is a transfinite cardinal (ℵ₀, ℵ₁, etc.) — irrelevant for computational use.

**Computational meaning.**
For a concrete finite set: count elements. O(1) if the size is tracked, O(|S|) if it
must be computed. As a constraint, `|S| = n` or `|S| ≤ n` or `|S| ≥ n` bounds the
size of a set variable. This is crucial for optimization problems ("find a subset of
size exactly k satisfying...").

**Type signature.** `Set α → Nat`

**Returns.** A natural number (the element count).

**Notation.** `|S|`, `#S`, `card(S)`, `count(S)` (functional)

**Common in constraint/logic programming?** Yes. Cardinality constraints (`count { x ∈ S | P(x) } = k`)
are among the most useful operations in practical constraint modeling. In Evident, this
maps to `count { ... }` in the proposed quantifier vocabulary.

**Natural inverse.** Cardinality is not invertible (many sets share the same size), but
cardinality constraints are bidirectional: `|S| = 3` propagates both "S has at most 3
elements" and "S has at least 3 elements."

---

## 12. Empty Set: ∅

**Mathematical meaning.**
The unique set with no elements: `∅ = {}`. For any set S, `∅ ⊆ S`.

**Computational meaning.**
The empty collection. Serves as an identity element for union (`S ∪ ∅ = S`), an
annihilator for intersection (`S ∩ ∅ = ∅`), and the base case for recursive set
operations. Testing `S = ∅` is the natural termination condition for set iteration.

**Type signature.** `Set α` (a constant/constructor)

**Returns.** The empty set of any element type.

**Notation.** `∅`, `{}`, `empty`

**Common in constraint/logic programming?** Yes — constant. `∅` as a base case appears
in every recursive set definition. `S ≠ ∅` ("S is non-empty") is a common constraint.
In Evident, `none x in S : true` tests emptiness.

**Natural inverse.** None (it is a value, not an operation). But `S ≠ ∅` has the
inverse `∃ x ∈ S : true`.

---

## 13. Singleton: {x}

**Mathematical meaning.**
The set containing exactly one element: `{x} = { y | y = x }`. Note that `{x} ≠ x`
in general — a set containing one element is not the same as that element.

**Computational meaning.**
A unit collection. Lifts a value into the set world. Useful for building sets
incrementally (`S ∪ {x}` adds one element) and for expressing "exactly one thing
satisfies P" via `{ x ∈ S | P(x) } = {w}` for some witness w.

**Type signature.** `α → Set α`

**Returns.** A set containing exactly the given element.

**Notation.** `{x}`, `singleton(x)`

**Common in constraint/logic programming?** Moderately. Most useful as a constructor
and in uniqueness constraints. In Prolog/Datalog, a ground atom `fact(a)` is implicitly
the singleton `{a}` in the extension of `fact`.

**Natural inverse.** `{x}` can be deconstructed to recover `x` (since the set has
exactly one element). The inverse of singleton is "the unique element of" — well-defined
precisely when the set is a singleton.

---

## 14. Set Comprehension / Builder Notation: { x ∈ S | P(x) }

**Mathematical meaning.**
The subset of S satisfying predicate P:
`{ x ∈ S | P(x) } = { x | x ∈ S ∧ P(x) }`.
The restriction `x ∈ S` is required (by the axiom schema of separation) to avoid
Russell's paradox.

**Computational meaning.**
Filter a source set by a predicate. Computationally this is a loop with a test:
for each element of S, include it in the result iff P holds. Lazy versions defer
this computation; constraint-based versions propagate bounds.

This is one of the most powerful operations: it converts a predicate into a set,
enabling all set operations to apply to the result.

**Type signature.** `(Set α, α → Bool) → Set α`

**Returns.** The subset of S where the predicate holds.

**Notation.** `{ x ∈ S | P(x) }`, `{ x in S | P(x) }` (ASCII); in many languages `filter(P, S)`

**Common in constraint/logic programming?** Essential. In Datalog, a rule with a body
is a comprehension: `derived(x) :- base(x), P(x)` defines `derived = { x ∈ base | P(x) }`.
In Evident, comprehension is the primary way to build constrained collections.

**Natural inverse.** Yes: comprehension has a natural bidirectional reading. If `R = { x ∈ S | P(x) }`,
then `x ∈ R` ↔ `x ∈ S ∧ P(x)`. The solver can use this to propagate constraints on
elements back from the result set to the source set.

---

## 15. Image of a Function: f(S) = { f(x) | x ∈ S }

**Mathematical meaning.**
The set of all values produced by applying f to elements of S:
`f(S) = { f(x) | x ∈ S } = { y | ∃ x ∈ S : f(x) = y }`.
Also called the direct image or forward image of S under f.

**Computational meaning.**
Map and deduplicate: apply f to each element, collect the distinct results. Cost
O(|S|) for the mapping plus deduplication. As a constraint, if `y ∈ f(S)` and f is
known, then there must exist some `x ∈ S` mapping to y — an existential constraint.

**Type signature.** `(α → β, Set α) → Set β`

**Returns.** Set of all outputs.

**Notation.** `f(S)`, `f[S]`, `image(f, S)`, `map(f, S)` (functional)

**Common in constraint/logic programming?** Yes, especially when reasoning about
derived attributes. In Evident: `{ w.department | w ∈ workers }` is the image of
the `department` accessor over the workers set — the set of departments that have
at least one worker.

**Natural inverse.** The preimage (operation 16). `f` and `f⁻¹` form a Galois connection.

---

## 16. Preimage / Fiber: f⁻¹(v) = { x ∈ S | f(x) = v }

**Mathematical meaning.**
The set of all elements in S that map to value v under f:
`f⁻¹(v) = { x ∈ domain(f) | f(x) = v }`.
Also called the fiber over v or the inverse image. Note: this is not the inverse
function — it is always well-defined even when f is not injective.

**Computational meaning.**
Group-by: collect all elements of S sharing the same output value. In databases this
is a GROUP BY or WHERE clause. In constraint solving, preimage constraints say "x
must be among those mapping to v under f" — which can dramatically narrow x's domain.

**Type signature.** `(α → β, β, Set α) → Set α`

**Returns.** All elements of the domain set that produce the given output.

**Notation.** `f⁻¹(v)`, `preimage(f, v, S)`, `fiber(f, v)` (functional)

**Common in constraint/logic programming?** Very common implicitly. Database lookup
by foreign key is a preimage operation. In Evident, `{ w ∈ workers | w.department = d }` is
the preimage of `d` under the `department` accessor — all workers in department d.

**Natural inverse.** The image (operation 15). Together, image and preimage form the
Galois connection `f(S) ∋ y ↔ S ∩ f⁻¹(y) ≠ ∅`.

---

## 17. Partition: { S₁, S₂, ..., Sₙ }

**Mathematical meaning.**
A partition of S is a collection of non-empty, pairwise disjoint subsets whose union
is all of S:
- `Sᵢ ≠ ∅` for all i
- `Sᵢ ∩ Sⱼ = ∅` for i ≠ j
- `S₁ ∪ S₂ ∪ ... ∪ Sₙ = S`

**Computational meaning.**
Partitioning is grouping without overlap: every element ends up in exactly one group.
This appears as GROUP BY in SQL, color-assignment in graph coloring, bin-packing,
and assignment problems. As a constraint, it enforces that a collection of sets covers
a base set without redundancy — a combination of coverage and disjointness constraints.

**Type signature.** `(Set α, (Set (Set α))) → Bool` (verification) or `(Set α, α → β) → Set (Set α)` (construction by key)

**Returns.** A set of disjoint non-empty subsets that cover S.

**Notation.** `partition(S)`, `partition(S, f)` (partition by key function f)

**Common in constraint/logic programming?** Very common in scheduling, assignment, and
classification problems. The CP constraint `partition(S, S₁, ..., Sₙ)` enforces the
partition conditions all at once. In Evident, partition would be a named claim with
the three conditions as its decomposition.

**Natural inverse.** Reconstruction: given the partition `{S₁, ..., Sₙ}`, the original
set is `⋃Sᵢ`. The grouping key used to construct the partition may or may not be
recoverable.

---

## 18. Quotient Set: S/~

**Mathematical meaning.**
Given an equivalence relation ~ on S, the quotient set S/~ is the set of all
equivalence classes: `S/~ = { [x]~ | x ∈ S }` where `[x]~ = { y ∈ S | x ~ y }`.
The quotient set is a partition of S (by equivalence classes).

**Computational meaning.**
Union-Find (disjoint-set union) data structures compute quotient sets efficiently.
The quotient set collapses "equivalent" things into representatives. In type theory,
quotient types enforce that equivalent values are indistinguishable. In databases,
GROUP BY followed by aggregation is a quotient-like operation.

**Type signature.** `(Set α, α → α → Bool) → Set (Set α)` (where the relation must be an equivalence)

**Returns.** The set of equivalence classes (a partition of S).

**Notation.** `S/~`, `quotient(S, ~)`

**Common in constraint/logic programming?** Moderately. Useful when multiple distinct
representations correspond to the same "semantic" object (e.g., fractions: 1/2 ~ 2/4).
In constraint solving, symmetry-breaking constraints are effectively quotient operations
— they collapse symmetrically equivalent solutions into one canonical representative.

**Natural inverse.** The canonical surjection `π : S → S/~` sending each element to
its class. Given a class `[x]~`, any element of the class is a preimage under π.

---

## 19. Disjoint Union (Tagged Union): S ⊔ T

**Mathematical meaning.**
A union that tracks which set each element came from, even when S and T overlap:
`S ⊔ T = { (s, L) | s ∈ S } ∪ { (t, R) | t ∈ T }`.
Unlike ordinary union, elements of S ⊔ T are tagged. If S and T are already disjoint
and no confusion is possible, `S ⊔ T = S ∪ T`. But the tagged version always works.

**Computational meaning.**
Sum types (variant types, tagged unions, `Either` in Haskell, `Result` in Rust).
The computational content is: every element carries a label saying where it came from.
This enables case analysis — you can always tell whether you're looking at a "left"
or "right" element.

**Type signature.** `(Set α, Set β) → Set (Left α | Right β)` (heterogeneous) or `(Set α, Set α) → Set (Tag × α)` (homogeneous)

**Returns.** A set of tagged elements, preserving origin information.

**Notation.** `S ⊔ T`, `S + T` (type theory), `Either S T` (functional), `disjoint_union(S, T)`

**Common in constraint/logic programming?** Moderately. Appears naturally when modeling
choices between incompatible alternatives. In type theory, sum types are ubiquitous.
In constraint programming, disjunctive constraints are the constraint analog: either
condition A holds (Left) or condition B holds (Right).

**Natural inverse.** Case analysis (pattern matching). Given `x ∈ S ⊔ T`, check the
tag: if Left, recover the S-element; if Right, recover the T-element.

---

## 20. Relation Composition: R ∘ S

**Mathematical meaning.**
Given relation `S ⊆ A × B` and `R ⊆ B × C`, their composition is:
`R ∘ S = { (a, c) | ∃ b : (a, b) ∈ S ∧ (b, c) ∈ R } ⊆ A × C`.
(Some sources write `S ; R` to match function composition left-to-right.)

**Computational meaning.**
A join over the shared intermediate type B. In relational algebra this is the natural
join followed by projection. In graph terms, if S and R are edge sets, R ∘ S is the
set of pairs (a, c) connected by a two-hop path a → b → c.

**Type signature.** `(Rel B C, Rel A B) → Rel A C`

**Returns.** A new relation over the outer types A and C.

**Notation.** `R ∘ S`, `R ; S`, `compose(R, S)`

**Common in constraint/logic programming?** Yes, implicitly. Every multi-hop lookup in
a Datalog rule is a relation composition. In Evident, chained field access `w.department.name`
composes the `department` and `name` relations.

**Natural inverse.** The converse relation: `R⁻¹ = { (b, a) | (a, b) ∈ R }`. The
composition `R⁻¹ ∘ R` gives a reflexive relation on the image of R; `R ∘ R⁻¹` gives
one on the domain.

---

## 21. Transitive Closure: R⁺ and R*

**Mathematical meaning.**
- `R⁺` (transitive closure): `(a, b) ∈ R⁺` iff there exists a path `a = x₀, x₁, ..., xₙ = b`
  with `n ≥ 1` and `(xᵢ, xᵢ₊₁) ∈ R`.
- `R*` (reflexive-transitive closure): same but allows `n = 0` (includes the identity).

In other words: `R⁺` is "reachable in one or more steps"; `R*` is "reachable in zero
or more steps."

**Computational meaning.**
This is the graph reachability problem. BFS or DFS from every node gives R⁺ in O(V + E).
In Datalog, recursive rules naturally compute R⁺ — it is the canonical recursive query.
Semi-naïve evaluation and magic sets are the standard algorithms for computing it efficiently.

**Type signature.** `Rel α α → Rel α α`

**Returns.** A new relation on the same type, extended with all transitive paths.

**Notation.** `R⁺`, `R*`, `closure(R)`, `reachable(R)` (functional)

**Common in constraint/logic programming?** Extremely common. Graph reachability,
ancestor relationships, type hierarchy traversal, dependency analysis — all are
transitive closures. In Evident, the `reachable` example in the language design
documentation computes exactly R⁺ via recursive claims. This is perhaps the most
important recursive set operation in practice.

**Natural inverse.** None in general (you cannot uniquely recover R from R⁺). But
if R⁺ is given and you want a minimal R that generates it (the "base" of the
transitive closure), that is the transitive reduction — well-defined for DAGs.

---

## 22. Projection: π_A(R)

**Mathematical meaning.**
Given a relation `R ⊆ A₁ × A₂ × ... × Aₙ` and a subset of attributes `A ⊆ {A₁,...,Aₙ}`,
the projection `π_A(R)` is the set of tuples restricted to the columns in A:
`π_A(R) = { t|_A | t ∈ R }` where `t|_A` is `t` restricted to the attributes in A.

**Computational meaning.**
SELECT in SQL: pick certain columns, discard the rest, deduplicate. Projection is how
you extract relevant information from a relation while ignoring irrelevant fields. It
corresponds to existential quantification over the dropped dimensions: `π_{A}(R)` 
contains tuple `t` iff `∃ values for the other attributes : the full tuple is in R`.

**Type signature.** `(Rel (A₁ × ... × Aₙ), AttributeSet) → Rel (subset of attributes)`

**Returns.** A relation on fewer attributes, with duplicates removed.

**Notation.** `π_A(R)`, `project(R, A)`, `SELECT A FROM R` (SQL)

**Common in constraint/logic programming?** Very. In Datalog, every rule that mentions
fewer variables in the head than in the body is a projection. In Evident, accessing
a field of a record (`w.id`) is a projection of the worker relation onto the id attribute.

**Natural inverse.** Extension (adding back attributes). But projection is information-
losing in general — you cannot recover the dropped columns from the projected result
alone. The inverse is the preimage under projection: all full tuples that project to
a given partial tuple.

---

## Summary and Patterns

### Operations by computational character

**Decision procedures** (test membership, equality, containment):
∈, ∉, ⊆, ⊂, =

**Set constructors** (build new sets from old):
∪, ∩, \, △, ×, 𝒫, { x ∈ S | P(x) }, f(S), f⁻¹(v), S ⊔ T

**Structural operations** (reorganize elements):
partition, quotient, projection, disjoint union

**Relational operations** (follow connections):
R ∘ S, R⁺, R*, π_A(R)

**Primitive constants**:
∅, {x}

**Numeric measurements**:
|S|

### Operations by constraint-programming relevance

**Foundational** (every system needs these):
∈/∉ (membership), ∩ (intersection as domain narrowing), { x ∈ S | P(x) } (comprehension),
|S| (cardinality), R⁺ (transitive closure for reachability), ∅ (base case), ∪ (alternative rules)

**Important** (commonly needed in real problems):
⊆ (subset for type/containment checks), f(S) (image/map), f⁻¹(v) (preimage/lookup),
partition (for scheduling, assignment), π_A(R) (projection for field access), R ∘ S (join/chaining)

**Specialized** (needed for particular domains):
× (Cartesian product, usually implicit in relation representation), S ⊔ T (disjoint union
for variant types), S/~ (quotient for symmetry breaking), 𝒫(S) (power set for subset search),
△ (symmetric difference for change detection), Sᶜ (complement/negation), {x} (singleton)

---

## Summary Table

| Symbol | Name | Input Types | Output Type | Description |
|--------|------|-------------|-------------|-------------|
| `x ∈ S` | Membership | `(α, Set α)` | `Bool / Prop` | Tests or constrains that x belongs to S |
| `x ∉ S` | Non-membership | `(α, Set α)` | `Bool / Prop` | Tests or constrains that x does not belong to S |
| `S ⊆ T` | Subset | `(Set α, Set α)` | `Bool / Prop` | Every element of S is also in T |
| `S ⊂ T` | Strict subset | `(Set α, Set α)` | `Bool / Prop` | S ⊆ T and S ≠ T |
| `S ⊇ T` | Superset | `(Set α, Set α)` | `Bool / Prop` | T ⊆ S; S contains all of T |
| `S = T` | Set equality | `(Set α, Set α)` | `Bool / Prop` | S and T contain exactly the same elements |
| `S ∪ T` | Union | `(Set α, Set α)` | `Set α` | All elements in S, T, or both |
| `S ∩ T` | Intersection | `(Set α, Set α)` | `Set α` | Elements belonging to both S and T |
| `S \ T` | Difference | `(Set α, Set α)` | `Set α` | Elements of S that are not in T |
| `S △ T` | Symmetric difference | `(Set α, Set α)` | `Set α` | Elements in S or T but not both |
| `Sᶜ` | Complement | `(Set α, Set α)` | `Set α` | All universe elements not in S |
| `S × T` | Cartesian product | `(Set α, Set β)` | `Set (α × β)` | All ordered pairs (s, t) with s∈S, t∈T |
| `𝒫(S)` | Power set | `Set α` | `Set (Set α)` | The collection of all subsets of S |
| `\|S\|` | Cardinality | `Set α` | `Nat` | The number of elements in S |
| `∅` | Empty set | — | `Set α` | The unique set with no elements |
| `{x}` | Singleton | `α` | `Set α` | A set containing exactly one element |
| `{ x ∈ S \| P(x) }` | Comprehension | `(Set α, α → Bool)` | `Set α` | Subset of S satisfying predicate P |
| `f(S)` | Image | `(α → β, Set α)` | `Set β` | All outputs produced by applying f to elements of S |
| `f⁻¹(v)` | Preimage / fiber | `(α → β, β, Set α)` | `Set α` | All elements of S that map to v under f |
| `{S₁,...,Sₙ}` | Partition | `(Set α, α → β)` | `Set (Set α)` | Disjoint exhaustive subsets of S (grouped by key) |
| `S/~` | Quotient set | `(Set α, α → α → Bool)` | `Set (Set α)` | Set of equivalence classes under relation ~ |
| `S ⊔ T` | Disjoint union | `(Set α, Set β)` | `Set (Left α \| Right β)` | Tagged union preserving origin of each element |
| `R ∘ S` | Relation composition | `(Rel B C, Rel A B)` | `Rel A C` | All (a,c) pairs connected via some intermediate b |
| `R⁺` | Transitive closure | `Rel α α` | `Rel α α` | All pairs reachable by one or more R-steps |
| `R*` | Reflexive-transitive closure | `Rel α α` | `Rel α α` | All pairs reachable by zero or more R-steps |
| `π_A(R)` | Projection | `(Rel (A₁×...×Aₙ), AttributeSet)` | `Rel A` | Relation restricted to attribute subset A, deduplicated |
