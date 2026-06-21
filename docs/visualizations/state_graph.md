# State Graph

A node-link drawing of the **reachable state-transition graph** of an Evident
FSM: the directed graph whose nodes are states of the difference equation and
whose edges are single applications of the transition relation
`state = f(_state)`. This is the most literal rendering of "the program as a
dynamical system" — you see the actual graph the runtime walks, not a projection
or statistic of it.

The shared transition queries, variable ranking, and channel-assignment
primitives this method calls are defined in
[`00-core-machinery.md`](00-core-machinery.md). This document specifies only the
state-graph-specific algorithm.

---

## 1. What it shows

The question: **"What states can the program be in, and which states lead to
which?"** It answers reachability, cycle structure, fixed points (absorbing
states), and the branching factor of nondeterminism, all at once.

When to use it, by program shape:

- **Discrete** (every interface variable is `bool` / `enum` / `string`): the
  state space is finite, so the *exact, complete* reachable graph can be drawn.
  This is the ideal case — every node and every edge is ground truth.
- **Numeric or mixed** (at least one `int` / `real` interface variable): the
  reachable set is generally unbounded, so the full graph cannot be drawn.
  Instead a finite **sampled subgraph** is built from seeded trajectories and
  their nondeterministic fan-out, and laid out in real phase-space coordinates
  when ≥ 2 numeric leaves exist.

Use a state graph when the *structure of transitions* is the message. For
purely metric questions (basins, divergence) a phase portrait is better; the
state graph is for topology — cycles, sinks, connectivity, fan-out.

---

## 2. The object

Formally the rendered object is a directed graph `G = (V, E)`:

- `V` ⊆ the state space `S` = the Cartesian product of the interface variables'
  domains. Each `v ∈ V` is one complete state vector (an assignment of every
  carried interface variable).
- `E ⊆ V × V`: `(a, b) ∈ E` iff `b` is a successor of `a`, i.e. the transition
  relation `f` admits `b` as a next state when `a` is pinned as the previous
  state. The graph is the **functional graph** of the (possibly set-valued) map
  `f`.

Two derived structural facts are surfaced visually:

- **Terminal / absorbing states**: a node whose only successor is itself (a
  fixed point of `f`) or which has no successor at all. These are drawn with a
  distinct ring.
- **Self-loops** (`a ∈ f(a)`): drawn as a curved arrow back to the node, colored
  to stand out from ordinary edges.

In the discrete case `G` is exactly the reachable component of the functional
graph from the initial state. In the numeric case `G` is a finite subgraph of
it — a sampled approximation.

---

## 3. Inputs

From the core machinery (see `00-core-machinery.md` for definitions):

- `is_discrete()` — selects exact vs sampled construction.
- `initial_state()` — the first-tick state; the root of the BFS and a default
  seed.
- `reachable()` — `(states, edges)` exact reachable graph (discrete path).
- `successors(state, limit)` — the full set-valued image of one state (the
  nondeterministic fan), used for sampled BFS.
- `label(state)` — the human-readable tuple label `(v1, v2, …)` over the
  interface variables, used as node text.
- `state_vars` — ranked, deduplicated interface variables (axis order); supplies
  the first two numeric leaves for phase-space layout.
- `numeric_vars` / `categorical_vars` — the candidate channels for size / color.
- `enum_variants[name]` — the declared value domain of an enum variable, giving a
  stable color-legend order.

No transition relation is ever hardcoded; every edge comes from a solver query.

---

## 4. Algorithm

### 4a. Graph construction — discrete path

1. Call `reachable()` to get `(states, edges)`. Because every interface variable
   is finite-domain, this BFS terminates and is exact.
2. Build `G`: one node per state (carrying its `label(state)` and the raw state
   vector), one directed edge per `(from_index, to_index)` pair.

### 4b. Graph construction — numeric / mixed path

The reachable BFS would not terminate, so build a bounded sampled subgraph:

1. **Seed set.** Start with `initial_state()` (if any). For recognized program
   shapes, add a handful of hand-chosen phase-space seeds spread across the
   region of interest (e.g. points on a ring around the origin, to catch a limit
   cycle from inside and outside). For an unknown numeric IR, the initial state
   alone is the seed.
2. **Bounded BFS with deduplication.** Maintain `index : state_key → node_id`
   where `state_key` is the sorted `(name, value)` tuple list (canonical hashable
   form of a state). Pop a node from the FIFO frontier; query
   `successors(state, limit = fan_limit)` (the nondeterministic fan, capped);
   for each successor add a node (deduped via `index`) and an edge; enqueue
   unvisited successors. Stop when the frontier empties or `len(states)` reaches
   `max_nodes`.
3. **Isolated-seed guard.** Any seed that produced no edges is still added as an
   isolated node so it appears in the drawing.

Both paths produce the same `(G, states)` shape for the rest of the pipeline.

### 4c. Terminal classification

For every node `n`, compute its successor set in `G`. `n` is **terminal** iff
`succ(n) = ∅` (sink) or `succ(n) = {n}` (fixed point). These get the ring and a
distinct outline color.

### 4d. Layout

Two layout regimes, chosen by program type:

- **Phase-space layout** (numeric/mixed with ≥ 2 numeric leaves). Take the first
  two numeric interface variables `(ax, ay)` from `state_vars`. Place each node
  at its literal coordinate `(state[ax], state[ay])`. Edges then trace the *real
  trajectory* through phase space, so limit cycles and spirals are visible as
  geometry, not just topology. Axes are drawn and labeled with `ax`, `ay`.
- **Hierarchical layout** (discrete, or numeric with < 2 numeric leaves). Run a
  layered DAG layout — the **Sugiyama / Coffman–Graham** family, as implemented
  by Graphviz `dot`: assign nodes to ranks by longest-path layering, order
  within ranks to minimize edge crossings, assign x-coordinates to straighten
  edges. Fall back to a **force-directed (spring / Fruchterman–Reingold)** layout
  if the layered layout is unavailable. After a `dot` layout, scale x-coordinates
  by a constant factor (~2.6×) so wide tuple labels of same-rank siblings stop
  overlapping without distorting the rank structure.

### 4e. Rendering

1. **Figure scale.** Grow the canvas with node count so labels stay legible
   (e.g. width ≈ `clamp(node_count · k, w_min, w_max)`), with a wider aspect for
   the hierarchical case to give horizontal room for tuple labels.
2. **Node glyph sizing tiers.** Pick a base node area and font size by node
   count: large nodes + readable font for small graphs (≤ 30), medium for
   mid-size (≤ 80), tiny for large clouds (> 80). Above ~60 nodes, per-node text
   labels are dropped (in phase layout the *position is the label*).
3. **Edges.** Draw ordinary edges as arrows with a slight arc curvature (so
   antiparallel `a→b`, `b→a` pairs don't overlap). Draw self-loops separately as
   curved back-arrows in the highlight color.
4. **Terminal ring.** Every terminal node gets a heavy dark/highlight outline so
   fixed points and sinks pop regardless of fill color.

---

## 5. Variable → channel mapping

A node already encodes the *whole* state vector, so the layout consumes at most
two variables (the two phase-space axes, in the numeric case). The remaining
channels surface additional variables:

| Channel | Variable | Reasoning |
|---|---|---|
| **x, y position** | first two numeric leaves (`state_vars`), phase layout only | quantitative → position is the most effective encoding; lets edges trace real trajectories |
| **node color (hue)** | `categorical_vars[0]` | hue is excellent for categorical (enum/bool) discrimination; a legend maps each value → color |
| **node size (area)** | first *varying* `numeric_vars` not already used as an axis | size is a coarse quantitative channel; only applied when the graph is small enough (≤ ~120 nodes) for size differences to read |
| **outline ring** | terminal/absorbing status (derived) | redundant structural emphasis, independent of the data channels |

This follows the standard effectiveness ranking (position ≻ size for
quantitative; hue for categorical): categorical variables go to color, the most
quantitative remaining variables go to position and then size.

**Color domain construction.** If the chosen categorical is an enum with a
declared `enum_variants` list, use that order (stable, complete legend).
Otherwise collect the values actually present; for booleans, force a fixed
`false`-then-`true` order. Fold in any value present but not declared so no node
goes uncolored. Map the domain onto a qualitative palette (`tab10` for ≤ 10
values, `tab20` otherwise) by index.

**Size domain construction.** Pick the first numeric variable whose values vary
across nodes (≥ 2 distinct). Min–max normalize its value `x` to
`t = (x − vmin)/(vmax − vmin)` and map to area in `[lo, hi]`. Skip if its only
candidate is already a layout axis.

---

## 6. Degradation & edge cases

- **No reachable states** (`initial_state()` is None, or empty graph): render a
  titled placeholder stating "no reachable states" rather than an empty canvas.
- **Discrete:** exact graph, hierarchical layout, per-node tuple labels — the
  high-fidelity case.
- **Numeric/mixed with ≥ 2 numeric leaves:** sampled subgraph in phase-space
  coordinates; the layout *is* the data.
- **Numeric/mixed with < 2 numeric leaves:** sampled subgraph in hierarchical
  layout (fall back to the discrete-style drawing).
- **No categorical variable** (pure numeric, e.g. an oscillator): color falls
  back to a two-tone scheme — one hue for ordinary states, the highlight hue for
  terminal states.
- **No second numeric / no varying numeric for size:** size channel is dropped;
  all nodes share the base size.
- **Large graph (> ~60–80 nodes):** drop per-node labels and shrink glyphs; the
  drawing degrades to a structural cloud (topology over identity).
- **Sampled graph never claims completeness:** the title states the construction
  mode (exact vs sampled) so the viewer knows whether absent edges mean
  "impossible" or merely "unsampled".

---

## 7. Parameters

| Parameter | Default | Meaning |
|---|---|---|
| `fan_limit` | 8 (4 for dense phase-space samples) | max successors taken per node in the sampled BFS (`successors` limit) |
| `max_nodes` | 300 (400 for recognized numeric shapes) | hard cap on sampled-graph node count |
| `steps` (seed trajectories) | 60–80 | nominal trajectory length for seeding (BFS cap dominates in practice) |
| `reachable` limit | 5000 | hard cap inside the exact BFS (core machinery) |
| label cutoff | 60 nodes | above this, per-node text labels are dropped |
| size-channel cutoff | 120 nodes | above this, the numeric size channel is disabled |
| base node area tiers | 1600 / 900 / 120 | for ≤ 30 / ≤ 80 / > 80 nodes |
| font size tiers | 8 / 6 / 4 | for ≤ 30 / ≤ 80 / > 80 nodes |
| x-stretch factor (dot layout) | 2.6 | horizontal spread to separate sibling labels |
| size area range | `[0.45·base, 1.7·base]` | min–max node area for the size channel |
| palette | `tab10` (≤ 10 values) / `tab20` | qualitative color map for the categorical channel |
| edge arc curvature | ~0.06 rad | so antiparallel edges separate |

---

## 8. References

- **Functional graphs / state-transition graphs** of finite dynamical systems —
  the reachable-graph object is the functional graph of the map `f`; sinks and
  fixed points are its absorbing nodes.
- **Layered graph drawing** — Sugiyama, Tagawa, Toda, *Methods for Visual
  Understanding of Hierarchical System Structures* (1981); rank assignment via
  Coffman–Graham; as implemented in Graphviz `dot` (Gansner et al., *A Technique
  for Drawing Directed Graphs*, 1993).
- **Force-directed layout** (fallback) — Fruchterman & Reingold, *Graph Drawing
  by Force-Directed Placement* (1991); Eades' spring model.
- **Breadth-first search** for reachability and the sampled subgraph frontier.
- **Visual encoding effectiveness** — Cleveland & McGill (1984); Mackinlay's
  ranking (1986); Bertin's *Semiology of Graphics* (1967) for the
  position-for-quantitative, hue-for-categorical mapping.
- **Phase-space embedding of trajectories** — placing nodes at their state
  coordinates so edges trace orbits; standard in dynamical-systems visualization
  (Strogatz, *Nonlinear Dynamics and Chaos*).
