# Observability and Comprehension Goals

Evident programs define constraint systems. The solver finds satisfying
assignments; the sampler generates a partial subset of the solution space via
Monte Carlo simulation. The visualisation layer exists to help users make sense
of what that sample set reveals about the system.

This document lists the comprehension goals we want to support, maps them to
rendering modes, and records the design rationale for each.

---

## What users are trying to understand

### 1. Extent
**Question:** What values can each variable actually take?

Users need to know the feasible range of each variable — not just the bounds
declared in the schema, but the effective range after all constraints interact.
A variable declared `∈ Nat` with no other constraints could theoretically be
0..∞, but other constraints usually shrink it significantly.

**Current support:** Variable range finder (min/max via Optimize + tightening),
axis spread in scatter plots, count bars for enums.

---

### 2. Correlation
**Question:** Do variables move together or independently?

When `priority` is high, is `duration` also high? When `src = 1`, which `dest`
values appear? Correlation reveals dependencies that aren't always obvious from
reading the constraints.

**Current support:** Scatter plot (numeric × numeric), strip plot (enum ×
numeric).

---

### 3. Relational structure
**Question:** What kind of relation is this?

A schema with two variables defines a binary relation. That relation may be:

- A **function** — each left-side value maps to exactly one right-side value
- **Injective** — no two left values share a right value
- **Surjective** — every right-side value is reached by at least one left value
- **Bijective** — both injective and surjective
- **Many-to-many** — neither injective nor surjective

These structural properties are invisible in a scatter plot. In a relation
diagram — two columns of nodes with directed arrows between them — they are
immediately obvious from the fan-out and fan-in of the arrows.

**Current support:** None.
**Planned:** Bipartite / relation diagram.

---

### 4. Graph topology
**Question:** What is the shape of the relation when it is closed on one set?

When both variables of a relation come from the same type (e.g. `src ∈ Nat`
and `dst ∈ Nat` both representing graph nodes), the relation defines a directed
graph. Users want to know:

- Is it connected? Are there isolated components?
- Are there cycles? Is it a DAG?
- What are the in-degree and out-degree distributions?
- Are there hubs (high-degree nodes)?
- What paths exist?

A force-directed node-link diagram answers all of these at a glance.

**Current support:** None. The scatter plot renders `(src, dst)` pairs as dots
at Cartesian coordinates, which looks like a matrix — not a graph.
**Planned:** Homogeneous graph (force-directed, single node set).

---

### 5. Reachability
**Question:** From a given element, what else is reachable through the relation?

Can you get from node 1 to node 4 through a sequence of edges? Are all nodes
reachable from a common root? Are there sink nodes with no outgoing edges?

**Current support:** None.
**Planned:** Homogeneous graph with path/component highlighting on hover.

---

### 6. Symmetry properties
**Question:** Is the relation reflexive, symmetric, or transitive?

Symmetric relations show bidirectional arrows; reflexive ones show self-loops;
transitive relations have a specific density pattern. These are structural
properties of the relation as a whole that emerge from looking at all samples
together.

**Current support:** None.
**Planned:** Homogeneous graph (bidirectional arrows, self-loops rendered
explicitly).

---

### 7. Density and coverage
**Question:** How much of the possible space appears in the sample?

Is the solution space sparse or dense? Are there conspicuous gaps — pairs that
never appear even after many samples? A gap might mean a constraint forbids that
combination, or just that it is rare.

**Current support:** Scatter plot (gaps as empty regions), count bars (zero-count
bars for enum values that never appear).
**Planned:** Relation diagram (missing arrows = unreachable pairs made explicit).

---

### 8. Conditional behavior
**Question:** When I fix one variable, what does the rest look like?

Pinning `priority = 5` and resampling shows how all other variables change.
This is the most direct way to probe a constraint system interactively.

**Current support:** Given-variable pinning in the schema panel, transfer
function sweep (POST /transfer).

---

### 9. Clustering
**Question:** Do solutions group together? Are there natural partitions?

Some schemas have solution spaces with multiple distinct clusters — regions of
solutions that are internally consistent but not connected to each other.
Knowing this exists changes how you reason about the system.

**Current support:** Scatter plot with color encoding reveals clusters when
they align with a variable's value.
**Planned:** Instance graph — samples as nodes, shared variable values as edges,
clusters emerge as connected components.

---

### 10. Schema composition
**Question:** How do multiple schemas relate to each other?

Which schemas extend which? Which schemas share variables (implicit joins)?
Which schemas are sub-relations of others?

**Current support:** None (the source text shows this, but no visual).
**Planned:** Schema dependency graph — static diagram drawn from the parse
result, shows extends/uses relationships between schemas.

---

## Rendering modes: capability matrix

| Goal | Scatter | Strip | Count bars | Relation diagram | Graph (homogeneous) | Instance graph | Schema graph |
|---|:---:|:---:|:---:|:---:|:---:|:---:|:---:|
| Extent | ✓ | ✓ | ✓ | | | | |
| Correlation | ✓ | ✓ | | | | | |
| Relational structure | | | | **✓✓** | | | |
| Graph topology | | | | | **✓✓** | | |
| Reachability | | | | | **✓✓** | | |
| Symmetry | | | | ✓ | **✓✓** | | |
| Density / gaps | ✓ | | ✓ | ✓ | ✓ | | |
| Conditional behavior | ✓ | ✓ | | | | | |
| Clustering | ✓ | | | | ✓ | **✓✓** | |
| Schema composition | | | | | | | **✓✓** |

---

## Annotation syntax

Plot type is declared in an `-- @plot` comment above the schema:

```
-- @plot type=scatter  x=duration  y=priority   color=status
-- @plot type=graph    x=src       y=dst
-- @plot type=relation x=input     y=output
```

When `type=` is omitted, the renderer auto-detects from variable types:
- numeric × numeric → scatter
- enum × numeric (or numeric × enum) → strip
- enum only → count bars
- same-type × same-type → **graph** (new)
- different-type × different-type → **relation diagram** (new)

---

## Design principles

**Show the sample set as a relation, not as points.** Each sample is one tuple
in a relation. The collection of all samples approximates the full relation. The
visualisation should make the relational structure visible, not just plot
individual tuples as dots.

**Let the types drive the layout.** If both variables come from the same type,
one node set. If from different types, two node sets. The schema already encodes
this information — the renderer should use it automatically.

**Accumulate across samples.** Like the scatter plot, graph diagrams should grow
as more samples arrive. Each new `(src, dst)` pair either adds a new edge or
reinforces an existing one (shown by edge weight / thickness).

**Make structure visible at a glance.** A user should be able to tell in under
five seconds whether a relation is a function, whether a graph is connected, and
whether there are hubs — without counting arrows or reading numbers.
