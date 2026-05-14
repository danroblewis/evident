# Generic types and claims in Evident

This is the design for adding generic type parameters to Evident.
The motivating example throughout is `Toposort` over a `Seq(Edge<T>)`
graph — see `docs/design/toposort.md` for why the current
representation hurts and how generics dissolve the problem.

## What we're adding

```evident
type Edge<T>(from, to ∈ T)

claim Toposort<T>
    items ∈ Seq(T)
    edges ∈ Seq(Edge<T>)
    sorted ∈ Seq(T)
    ...
```

Two distinct features, used together:

- **Generic types** — type declarations parameterized by type
  variables. `type Edge<T>(from, to ∈ T)`. Each unique
  instantiation (`Edge<Rect>`, `Edge<Effect>`, …) is a distinct
  type at the Z3 layer.
- **Generic claims** — claim declarations parameterized by type
  variables. `claim Toposort<T>` works for any `T`; the body
  references `T` to talk about whatever the caller picks.

Both also extend naturally to multiple type parameters:
`type Pair<A, B>`, `type Map<K, V>`. The doc focuses on the
one-param case because that's what Toposort needs; the design
generalizes.

## Why now

We've hit the same need three times in the last few days:
- `stdlib/toposort.ev` wants `Edge<T>` for any node type, not a
  per-type `RectDep`, `EffectDep`, …
- Mario's level work would benefit from `Optional<T>` for "this
  field is sometimes set" rather than sentinel values.
- The `Edge ≈ Map entry` observation: `type Edge<K, V>` (or
  `type MapEntry<K, V>`) IS what we'd want a `Map<K, V>` to use.

Per-type photocopies are about to multiply across the codebase.
Generics dissolve them.

## Syntax

### Type declaration

```evident
type Edge<T>(from, to ∈ T)

type Pair<A, B>
    first  ∈ A
    second ∈ B

type Optional<T>           -- (as enum)
    -- ...

enum Result<T, E> =
    | Ok(T)
    | Err(E)
```

Type parameters are introduced in angle brackets immediately
after the type name, comma-separated, single capital letters by
convention but any capitalized identifier works.

Inside the type body, the parameter names appear as ordinary
type references (`∈ T`, `∈ Seq(A)`, `∈ Pair<T, T>`).

### Type usage

Anywhere a type name appears, type arguments come in matching
angle brackets:

```evident
e ∈ Edge<Rect>                 -- field/param/local
xs ∈ Seq(Edge<Rect>)            -- nested in builtin container
ys ∈ Seq(Pair<Int, String>)     -- multiple args
```

Generic types without arguments are an error — `e ∈ Edge` is
not a valid use. (Unless we add bare-generic param support
like `Seq` has, see "Open questions" below.)

### Claim declaration

```evident
claim Toposort<T>
    items ∈ Seq(T)
    edges ∈ Seq(Edge<T>)
    sorted ∈ Seq(T)
    -- body uses T to relate items, edges, sorted
```

Same syntax — angle brackets after the claim name.

### Claim invocation

Two forms — explicit and inferred:

```evident
-- Explicit:
Toposort<Rect>(items ↦ my_rects, edges ↦ my_edges, sorted ↦ sorted_rects)

-- Inferred (T derived from arg types):
Toposort(items ↦ my_rects, edges ↦ my_edges, sorted ↦ sorted_rects)
```

For v1 we ship **explicit only**. Inference is a v2 extension
once the explicit path is stable.

### The disambiguation question

`<` and `>` are already comparison operators in expressions.
The parser disambiguates by **context**: in a *type position*
(after a type-name identifier in a `∈ Type` membership, in a
nested `Seq(Type)`, in a claim invocation right after the
claim name), `<` opens a type-argument list. In an *expression
position*, `<` is comparison.

The fundamental rule: capitalized identifiers are types,
lowercase are values. `Edge<Rect>` — both sides are
capitalized, this is type-instantiation. `n < 5` — `n` is
lowercase, this is comparison. `i < #items - 1` — `i`,
`#items` are values, comparison.

Edge case: `MyType < 5` — this is a comparison between a type
name (Bool? what?) and a number, and it's nonsense regardless.
The parser can disambiguate by checking what makes type-sense.

A small disambiguation risk: `Foo<A, B>` where the parser might
not know yet if it's about to see a type-arg list. The standard
solution (Rust's "turbofish") is to require special syntax
when context is genuinely ambiguous. We probably won't need it
for v1 since claim invocations have a known structure
(`Name(arg ↦ value, …)`), but we'll keep the option open.

## Semantics: monomorphization

When the translator encounters a generic instantiation, it
produces a **monomorphic copy** of the generic by substituting
the type arguments for the type parameters everywhere in the
body.

`Edge<Rect>` becomes (conceptually) a new top-level type:

```evident
-- generated internally — never appears in source:
type Edge_Rect(from ∈ Rect, to ∈ Rect)
```

Each unique `(generic_name, type_args)` pair gets exactly one
monomorphic copy. The Z3 datatype builder treats `Edge<Rect>`
and `Edge<Effect>` as completely separate datatypes with
distinct sorts.

This is the standard implementation strategy used by Rust, C++,
and most static languages. It avoids any runtime cost — every
generic instantiation compiles to direct, specialized Z3
constructs — at the price of code duplication in the IR (each
instantiation is a separate SchemaDecl after monomorphization).

For our scale, code duplication isn't a concern: a handful of
generic types times a handful of instantiations = tens of
monomorphic copies. Cheap.

### What gets substituted

Type variables appear in:
- Field type annotations (`from ∈ T`)
- Claim parameter type annotations
- Body Memberships (`local_var ∈ Seq(T)`)
- Body type references inside expressions (constructor calls
  `Edge(a, b)` need to know which Edge variant; record literals
  in arg position; etc.)

Substitution is **structural** — replace every occurrence of
the type variable with the concrete type. No name capture
issues to worry about: type variables don't shadow value
identifiers.

### Caching

Monomorphic copies are cached in a `HashMap<(String,
Vec<TypeRef>), SchemaDecl>` keyed by generic name + args. First
use of `Edge<Rect>` produces and caches; subsequent uses hit
the cache.

## AST changes

```rust
pub struct SchemaDecl {
    pub name: String,
    pub keyword: Keyword,
    pub type_params: Vec<String>,    // NEW: ["T"] or ["A", "B"]
    pub params: Vec<Membership>,
    pub body: Vec<BodyItem>,
    // ...
}
```

Type references everywhere need to carry their arguments. Today
field type names are `String`. We extend to a richer form:

```rust
pub struct TypeRef {
    pub name: String,
    pub args: Vec<TypeRef>,         // empty for non-generic types
}
```

Every `type_name: String` in the AST (`Membership.type_name`,
nested in `Seq(...)`, etc.) becomes `type_name: TypeRef`. This
is a big mechanical refactor across the AST and the translator
— most type-name lookups need to handle the recursive shape.

Backwards-compat for non-generic types is structural: a
non-generic reference is just a `TypeRef { name: "Int", args:
[] }`.

## Implementation phases

Six phases, each independently testable.

### Phase 1: Parser + AST shape

- Extend lexer to recognize `<` `>` in type contexts. Initial
  implementation: lookahead from after a capitalized identifier
  to see if `<` followed by another type-name-ish token
  appears.
- Parse `type Foo<T1, T2>(...)` declarations.
- Parse `Foo<Bar>` references in type position.
- Extend `SchemaDecl` with `type_params: Vec<String>`.
- Refactor type-name fields to `TypeRef`.
- All existing programs should continue to parse — non-generic
  types become `TypeRef { name, args: [] }`.

**Done when**: existing tests still pass; a smoke test
`type Foo<T>(x ∈ T)` + `f ∈ Foo<Int>` parses without errors
(may not yet translate).

### Phase 2: Monomorphization pass

A new pass that runs early in the load pipeline, after parsing
and before the existing inference passes (which would
otherwise see unsubstituted type variables).

```rust
fn monomorphize_generics(schemas: &mut HashMap<String, SchemaDecl>);
```

Algorithm:
1. Walk every body item in every schema, collect every type
   instantiation `(generic_name, type_args)`.
2. For each unique pair: substitute type_args → type_params in
   the generic's body, produce a new monomorphic schema, name
   it deterministically (e.g., `Edge<Rect>` → `Edge_Rect`,
   handling nesting like `Edge<Pair<Int, String>>` →
   `Edge_Pair_Int_String`).
3. Update all references in the calling schemas to point at
   the monomorphic name.
4. Iterate to fixed point — a monomorphic schema might itself
   contain generic instantiations that need to be expanded.
5. Add monomorphic schemas to the global `schemas` map.

**Done when**: a schema using `Edge<Rect>` and `Edge<Effect>`
translates to two distinct Z3 datatypes, each with the right
field types.

### Phase 3: Datatype builder + translator integration

- Update `translate/datatypes.rs` to operate on monomorphic
  schemas. (Mostly a no-op since monomorphization already
  produced concrete types — the builder just sees more
  schemas in the map.)
- Verify that nested generic uses like `Seq(Edge<Rect>)` and
  `Seq(Pair<Int, String>)` work — the Seq path looks up the
  element type by name, which now refers to the monomorphic
  copy.

**Done when**: a test program declares
`xs ∈ Seq(Edge<Rect>)`, populates it via per-index assignment,
and accesses `xs[i].from` returning a Rect.

### Phase 4: Generic claims

- Parse `claim Toposort<T>` (same syntax as generic types).
- Extend monomorphization to claims — when a claim invocation
  uses type arguments, monomorphize the claim's body.
- Claim invocation syntax `Toposort<Rect>(...)`.

**Done when**: a generic claim is invoked with explicit type
args and the body inlines correctly with the right
substitutions.

### Phase 5: Rewrite `stdlib/toposort.ev`

```evident
type Edge<T>(from, to ∈ T)

claim Toposort<T>
    items ∈ Seq(T)
    edges ∈ Seq(Edge<T>)
    sorted ∈ Seq(T)
    n ∈ Nat
    #items = n
    #sorted = n
    
    -- Permutation: each item appears once in sorted.
    -- (Encoded via position indices internally; see below.)
    position ∈ Seq(Int)
    #position = n
    ∀ i ∈ {0..n - 1} : 0 ≤ position[i] ∧ position[i] < n
    ∀ i ∈ {0..n - 1} : ∀ j ∈ {0..n - 1} :
        i < j ⇒ position[i] ≠ position[j]
    
    -- Edge constraints. For each edge, find the two items by
    -- value-equality and constrain their positions.
    ∀ k ∈ {0..#edges - 1} :
        ∀ i ∈ {0..n - 1} : ∀ j ∈ {0..n - 1} :
            (items[i] = edges[k].from ∧ items[j] = edges[k].to)
                ⇒ position[i] < position[j]
    
    -- Materialize: sorted[position[i]] = items[i].
    ∀ i ∈ {0..n - 1} : sorted[position[i]] = items[i]
```

The interface is `(items ∈ Seq(T), edges ∈ Seq(Edge<T>), sorted ∈ Seq(T))`
— domain types in, domain types out. The `position` Seq is an
internal implementation variable, invisible to callers.

**Caller**:

```evident
my_rects ∈ Seq(Rect)
my_edges ∈ Seq(Edge<Rect>)
sorted_rects ∈ Seq(Rect)
Toposort<Rect>(items ↦ my_rects, edges ↦ my_edges, sorted ↦ sorted_rects)
```

**Cost note**: the body has O(n²) ∀ blocks for the distinct
constraint and O(m · n²) for the edge constraint (where m =
#edges). Z3 handles this fine for the scales we care about
(tens of nodes); the alternative — keeping the `position`
indexed lookup as a public output — leaks indices, which we've
banned.

**Done when**: `stdlib/toposort.ev` uses generics, all
existing tests pass, plus new tests with Rect and Effect node
types.

### Phase 6: Polish + migration

- Update `runtime/tests/toposort.rs` to use the generic
  `Toposort<Rect>` form.
- Add stdlib generics that come up naturally:
  `type Pair<A, B>`, `type Optional<T>` (as enum),
  `type Map<K, V>` (probably as `Seq(MapEntry<K, V>)`).
- Document the generics feature in CLAUDE.md (a new section
  near "Generic Seq parameters").

## Worked example: Toposort under generics, end-to-end

**Generic definitions** (one place each, no per-type
photocopies):

```evident
type Edge<T>(from, to ∈ T)

claim Toposort<T>
    items ∈ Seq(T)
    edges ∈ Seq(Edge<T>)
    sorted ∈ Seq(T)
    -- body as in Phase 5 above
```

**Use site for Rect**:

```evident
import "stdlib/toposort.ev"

type Rect(pos, size ∈ IVec2)

claim render_order
    rects ∈ Seq(Rect)
    edges ∈ Seq(Edge<Rect>)
    sorted ∈ Seq(Rect)
    
    -- Caller defines the graph however they like.
    -- For example, render-order edges:
    #rects = 3
    rects[0] = Rect(IVec2(0, 0),   IVec2(640, 480))   -- sky
    rects[1] = Rect(IVec2(0, 400), IVec2(640, 80))    -- ground
    rects[2] = Rect(IVec2(100, 300), IVec2(120, 16))  -- platform
    
    #edges = 2
    edges[0] = Edge(rects[0], rects[1])   -- sky before ground
    edges[1] = Edge(rects[1], rects[2])   -- ground before platform
    
    Toposort<Rect>(items ↦ rects, edges ↦ edges, sorted ↦ sorted)
```

**Use site for Effect** (no per-type Toposort copy needed):

```evident
import "stdlib/toposort.ev"

claim effect_order
    effs ∈ Seq(Effect)
    edges ∈ Seq(Edge<Effect>)
    sorted ∈ Seq(Effect)
    -- ... edges describing "set_color before fill_rect", etc. ...
    Toposort<Effect>(items ↦ effs, edges ↦ edges, sorted ↦ sorted)
```

One stdlib definition; arbitrary number of element types.

## Open questions

These come up during design but don't need to be answered
before Phase 1 starts.

### 1. Bare-generic claim params

`claim Toposort(items ∈ Seq, ...)` already works today via
the bare-`Seq` generic-param feature — the claim is implicitly
polymorphic over Seq's element type, inferred at call sites.

How does this interact with explicit `<T>` type params?
Possibilities:
- Explicit type params win; bare-Seq becomes a deprecated
  shortcut.
- Both supported; explicit when you want to relate types
  across params (`Seq(T)` + `Seq(Edge<T>)` need the same T),
  bare-Seq when one Seq is opaque.

Probably the second. The bare-Seq is too useful as ergonomic
shortcut to deprecate.

### 2. Type inference at call sites

`Toposort(items ↦ my_rects, ...)` — should T be inferred from
`my_rects ∈ Seq(Rect)`?

Mechanically: at call site, look at the types of the bound
slots; unify against the generic's parameter types; solve for
T.

This is Hindley-Milner-lite. Doable but a real chunk of
implementation. v1 ships **explicit type args only**
(`Toposort<Rect>(...)`); inference is a v2 extension.

### 3. Default type arguments / type aliases

`type EdgeRect = Edge<Rect>` — a named alias for a specific
instantiation, useful for repeated use.

Nice-to-have, low priority. Defer.

### 4. Constraints on type parameters

`type Edge<T : Hashable>(from, to ∈ T)` — restrict T to types
that support some operation.

For Evident, Z3 datatype equality is universally available, so
there's no operation we'd need to require. **No.**

### 5. Higher-kinded types

`type Container<F<_>>` — F itself is a type constructor. Used
for `Functor`, `Monad`, etc. in some languages.

Out of scope. Probably forever.

### 6. Generic enums

`enum Result<T, E> = Ok(T) | Err(E)` should work the same way
as generic record types. Each variant's payload types can
reference type parameters. Monomorphization produces a
concrete enum per instantiation.

Yes, ship in v1 as part of Phase 1. The enum-handling code
already builds Z3 datatypes via similar machinery to records.

### 7. Recursive generics

`type LinkedList<T> = Nil | Cons(T, LinkedList<T>)` — self-
reference with a type param.

Z3 supports mutually-recursive datatypes (already used for
Cons/Nil). Generic recursion adds a wrinkle for
monomorphization: when expanding `LinkedList<Int>`, the body
references `LinkedList<Int>` itself, which is the type being
expanded. Fixed-point with cycle detection.

Yes, ship in v1 — it's needed for any practical typed-list use.

## Interactions with existing features

- **Passthrough `..Foo<T>`** — works the same as non-generic
  passthrough; the monomorphic copy of `Foo<T>` is what gets
  composed.
- **Names-match composition** — generic claims must be invoked
  with explicit type args; names-match doesn't infer them in
  v1.
- **Existing `Seq` bare-generic param** — coexists; see Open
  Question 1.
- **Subclaim dispatch** — subclaims of a generic claim inherit
  the type parameter scope.
- **lhs-eq type inference**, **chained-membership**,
  **prev-tick decl injection** — all unaffected, since these
  operate on monomorphized schemas by the time they run.

## Scope estimate

Phase 1 (parser + AST): 1–2 days. Mostly mechanical refactor
of type-name to TypeRef across the codebase. The disambiguation
work is the unknown.

Phase 2 (monomorphization): 1 day. New pass, well-understood
algorithm.

Phase 3 (datatype builder integration): 0.5 days. Verifying
nothing breaks, more than building new.

Phase 4 (generic claims): 1 day. Reuses Phase 2 machinery.

Phase 5 (stdlib/toposort.ev rewrite): 0.5 days plus debugging.

Phase 6 (polish + a couple of stdlib additions): 1 day.

**Total**: 4–6 days of focused work, depending on how many
edge cases the parser disambiguation reveals.

## Risk surface

Where things are most likely to go wrong:
- Parser disambiguation of `<` `>` — Rust burned a lot of
  cycles on this; we should be cautious.
- The TypeRef refactor touches every type-name in the AST,
  which is hundreds of sites. High volume, low individual risk
  per site, but easy to miss one and produce subtle bugs.
- Monomorphization cache key — getting the structural equality
  of `TypeRef` right (especially for nested cases like
  `Edge<Pair<Int, String>>`) matters for cache correctness.
- Recursive generics (Open Q 7) — fixed-point with cycle
  detection is easy to get subtly wrong.

Each is solvable; none is a blocker. Just need to be careful.
