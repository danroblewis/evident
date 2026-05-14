# Topological sort in Evident

This doc captures the design decisions around `stdlib/toposort.ev`
and — more importantly — the broader question it surfaces: how do
we represent a directed acyclic graph in a constraint language
that has no generic types and no first-class constraint values?

Toposort is a recurring need that will keep coming up:

- **Effect dispatch ordering.** When the user defines several
  `Seq(Effect)` chunks per tick, the runtime needs to interleave
  them respecting ordering constraints (set-color before
  fill-rect, all draws before present, etc.). Today this is
  encoded via the legacy `effects` slot's manual ordering or via
  alphabetical-prefix renames; toposort is the proper answer.
- **GLSL / shader codegen.** The historical GLSL transpiler used
  toposort to determine emission order from a dependency DAG.
- **General-purpose codegen.** Compiling Evident to Python, C,
  Rust, or assembly all share the same pattern: sort operations
  by data dependency, emit in order. Self-hosting builds on this.
- **Level placement and build pipelines.** Any "these come
  before those" partial order that needs to be materialized as
  a sequence.

Toposort is "the constraint primitive that turns a partial order
into a total order." It deserves a clean home.

## The representation problem

A DAG is mathematically `(V, E)`: a set of vertices and a set of
ordered pairs from `V × V`. The pairs identify their endpoints
*by reference to vertex identity*. In Evident — a constraint
language with values, not objects, and no built-in identity —
there is no built-in answer to "which vertex is this." That's the
representation question.

Four options, considered explicitly:

### Option 1: Separate Edge type, edges as a Set (chosen)

```evident
type Rect
    pos, size ∈ IVec2

type RectDep(from ∈ Rect, to ∈ Rect)

claim Toposort
    items ∈ Seq(Rect)
    edges ∈ Set(RectDep)
    sorted ∈ Seq(Rect)
```

The Edge type sits alongside the domain type. Each call site
defines `<Node>Dep` (`RectDep`, `EffectDep`, `TaskDep`, …) — one
per node type. Toposort is also per-type. Vertices are
identified via Z3 value equality: two `Rect` values that compare
equal are the same vertex.

**Pros**:
- Domain types in, domain types out — no indices at the interface.
- The graph reads as a graph: a set of pairs of nodes.
- No parallel Seqs.
- Composable: callers can add metadata to the Edge type (weight,
  label, kind) without changing the toposort signature.

**Cons**:
- Generics-by-photocopy. Every node type wants its own Edge type
  and Toposort definition.
- Identity-by-value means two structurally-equal Rects are the
  same vertex. Distinct vertices need distinct values (often
  trivially true in practice; pathological when not).

### Option 2: Composed wrapper, graph embedded in the node

```evident
type Rect
    pos, size ∈ IVec2

type RectWithEdges
    ..Rect
    deps ∈ Set(Rect)   -- this rect depends on these (must come after)

claim Toposort
    items ∈ Seq(RectWithEdges)
    sorted ∈ Seq(RectWithEdges)
```

Each node carries its own dependency list via `..Rect`
passthrough composition. `Rect` doesn't change; `RectWithEdges`
is a *derivation* that adds graph context.

**Pros**:
- One Seq in, one Seq out — no separate edges argument.
- The domain type stays clean (composition, not modification).
- Natural when deps are intrinsic to the domain (task graphs,
  expression trees, render layers).

**Cons**:
- Forces the caller to wrap every node.
- Per-node deps duplicate edge information (each edge appears in
  at least one node's deps).
- Toposort body has to relate items by value-equality to resolve
  what's in each `deps` set; same identity-by-value caveat as
  option 1.

### Option 3 (rejected): Parallel Seqs

```evident
-- Don't.
items ∈ Seq(Rect)
edges_from ∈ Seq(Rect)
edges_to ∈ Seq(Rect)
```

The bare-`Seq` generic pattern would make this work for any node
type with no per-type definitions. We rejected it anyway because
**parallel Seqs are forbidden** in Evident (see CLAUDE.md). Z3
silently fills in unconstrained values, so any misalignment
between `edges_from` and `edges_to` produces a SAT solution that
looks valid but isn't. The structural invariant "these are
paired" can't be enforced by the type system, only by hand-
written length constraints, and missing constraints in Evident
are silent bugs.

### Option 4 (rejected): Int indices at the interface

```evident
-- Don't.
claim Toposort
    items ∈ Seq(Rect)
    edges ∈ Set(Edge)        -- Edge(from, to ∈ Int) — indices into items
    position ∈ Seq(Int)       -- where each item lands
```

This is what `stdlib/toposort.ev` currently does. The output is
indices; every caller has to invert the permutation to recover
the sorted items. Rejected for the user-facing interface — see
CLAUDE.md "Indices in interfaces are a leak." Indices are fine
inside the implementation; they don't belong in the contract.

## What's missing for a clean answer

The reason none of the today-realizable options is great:
**Evident lacks two language features** that would dissolve the
problem.

### Generic record types

```evident
-- Doesn't exist today:
type Edge<T>(from, to ∈ T)

claim Toposort<T>
    items ∈ Seq(T)
    edges ∈ Set(Edge<T>)
    sorted ∈ Seq(T)
```

One Edge type, one Toposort, works for any node type. The
per-type-Edge cost goes away. Payoff extends way beyond toposort
— `Pair<A, B>`, `Optional<T>`, `Result<T, E>`, `Map<K, V>`, etc.

### Higher-order claims (claim-as-value)

The deeper feature is passing constraint systems as parameters
— the Evident analog of anonymous functions / lambdas. In other
languages you'd write:

```python
sort(rects, key=lambda r: r.z_order)
topo_sort(tasks, deps=lambda t: t.prerequisites)
```

The Evident shape:

```evident
claim z_order(a ∈ Rect, b ∈ Rect)
    a.pos.y < b.pos.y

claim Toposort
    items ∈ Seq
    precede ∈ Claim(_, _)   -- claim-typed parameter
    sorted ∈ Seq

-- Usage:
Toposort(items ↦ my_rects, precede ↦ z_order, sorted ↦ sorted_rects)
```

`precede` is a claim parameter. Toposort's body instantiates it
inside the loop. The caller's `z_order` (or any 2-arg Bool
claim) gets inlined.

This is bigger than generics — it adds a value kind (`ClaimRef`)
and runtime machinery to look up and inline claims at expansion
time. But it's the right level of abstraction for parameterizing
constraint operations.

**Both features have wider payoff than toposort.** Generics
unlocks every typed container; higher-order claims unlocks every
operation parameterized by a predicate, comparator, or
transformation (`Sort`, `Filter`, `Map`, `Fold`, `EffectOrdering`,
…). They're real language work, not toposort-specific tweaks.

The design for generics — the path we're taking first — lives in
[`docs/design/generics.md`](generics.md), with toposort as the
worked end-to-end example.

## Decision

For now: **Option 1 (separate Edge type, per node type)**. It's
the cleanest of the today-available representations:
- Domain types at the interface (no indices).
- No parallel Seqs.
- Composable (Edge can carry metadata).
- Identity by Z3 value equality (acceptable; distinct vertices
  need distinct values, which is the usual case).

`stdlib/toposort.ev`'s current Int-indexed form is interim. The
underlying constraint encoding (position-as-permutation,
in-range, distinct, edge-respecting) is right; the *interface*
is wrong because we had no domain type to pass through. As
domain-specific use cases land (effect ordering, level layout,
codegen), each one will define its own Edge type and Toposort
wrapper, e.g. `ToposortRect`, `ToposortEffect`, `ToposortNode`.

The duplication will get noisy. That noise is the signal that
generics and/or higher-order claims are ready to be implemented.
The first real win for the new feature is folding the toposort
photocopies back into a single generic claim.

## Edge ≈ Map note

An `Edge(from, to)` is structurally the same as a `Map` entry
`(key, value)`. The two-field "this is associated with that"
shape generalizes. If we ever want a `Map<K, V>` in stdlib, it
shares its element type with `Edge` — the same record can serve
both, and operations like "extract all `to` values for a given
`from`" become a natural operation on either.

This is incidental for toposort but worth noting: any future
"associative collection" work doesn't need a new type; it reuses
`Edge` (or its successor under generics).

## Current state

- `stdlib/toposort.ev` implements Toposort over Int-indexed
  edges. Public API uses `n ∈ Nat` + `edges ∈ Seq(Edge)` + output
  `position ∈ Seq(Int)`. This is interim; see "interface leak"
  caveat above.
- `runtime/tests/toposort.rs` exercises the runtime-calling-stdlib-
  claim seam: load the file, invoke via `rt.query`, decode the
  result. The seam itself is generic; same pattern will reuse for
  effect-ordering, codegen, etc.
- No call site of Toposort in the rest of the codebase yet.
  Mario's display fsm uses alphabetical-prefix renames
  (`a_sky_effs`, `b_plat_effs`, …) as a stopgap; the right
  long-term fix is an effect-ordering claim that emits edges and
  a Toposort that consumes them.

## Migration path

When we implement either generics or higher-order claims:

1. Decide which (probably higher-order claims first — broader
   payoff, doesn't require monomorphization).
2. Rewrite `stdlib/toposort.ev` against the new feature.
3. Migrate per-type wrappers (when they exist) back to the
   single generic claim.
4. Wire effect-ordering into the runtime's dispatch path.

Until then: write per-type wrappers as needed, accept the
photocopy cost, point each new instance at this doc.
