# Morse Graph (Recurrence Skeleton)

A reimplementable specification of the **Morse graph** visualization for an Evident
program. The Morse graph is the *condensation DAG* of the program's reachable
transition graph: one node per strongly-connected component (SCC), edges encoding
the gradient-like flow between recurrent sets. It is the single most compact summary
of "where the dynamics settle, where they originate, and how they pass through."

Shared primitives (transition queries, variable ranking, channel mapping) are
defined in **`docs/visualizations/00-core-machinery.md`**; this doc references them
by name and does not re-derive them.

---

## 1. What it shows

The Morse graph answers: **what is the global recurrence structure of the
difference equation?** It collapses every cyclic region of state space into a single
node and shows the partial order of irreversible flow between those regions:

- **Attractors** — recurrent sets the system falls into and never leaves (sinks of
  the flow; fixed points and limit cycles live here).
- **Repellers** — recurrent or single-state sets the flow only ever leaves (sources).
- **Transients** — everything the flow merely passes through on its way from a
  repeller to an attractor.

Use it for **any program shape**:

- **Discrete / mixed** (finite reachable set): gives the *exact* recurrence
  structure — terminal states, cycles, the reachability skeleton.
- **Pure numeric** (e.g. an oscillator with a limit cycle): gives an *approximate*
  recurrence skeleton via grid-sampled quantized flow; the limit cycle appears as a
  nontrivial attracting SCC.

It is the right pick when the question is qualitative ("is there a cycle? a unique
attractor? a stuck state?") rather than metric. For metric/geometric detail of the
flow, use a phase portrait or nullcline field instead.

---

## 2. The object

Formally, given the transition relation `T ⊆ S × S` over the (finite or sampled)
state set `S`:

1. Build the directed graph `G = (V, E)` where `V` is the reachable (or sampled)
   state set and `(u, v) ∈ E` iff `v` is a successor of `u` under `T`.
2. Compute the strongly-connected components of `G`. The **condensation**
   `C = G / ∼` (quotient by the SCC equivalence) is a DAG: its nodes are SCCs, with
   an edge `[u] → [v]` iff some `G`-edge crosses from SCC `[u]` to SCC `[v]`.
3. The Morse graph is `C`, with each node classified by its degree in `C` and its
   internal size:
   - **size** = number of original states in the SCC.
   - **is_cycle** = `size > 1`, OR `size == 1` with a self-loop in `G` (a fixed
     point: a recurrent set of one state).
   - **role** by condensation degree:
     - out-degree 0, in-degree > 0 → **attractor**
     - in-degree 0, out-degree > 0 → **repeller**
     - in-degree 0 AND out-degree 0 → **isolated**
     - otherwise → **transient**

This is exactly the **Conley–Morse decomposition** of a dynamical system, restricted
to a finite/sampled graph: recurrent components (the Morse sets) ordered by a partial
order (the flow). The classical continuous version partitions an invariant set into
isolated invariant sets connected by gradient-like flow; on a finite graph the
recurrent sets are precisely the nontrivial SCCs and the order is the condensation
DAG.

---

## 3. Inputs

From the core machinery (`00-core-machinery.md`):

- `is_discrete()` — branch selector (all interface vars are bool/enum/string).
- `initial_state()` — the first-tick seed.
- `successor(state)` — one step of the map (used in numeric sampling chains).
- `successors(state)` — set-valued image (used by `reachable()` for the fan).
- `reachable(limit)` — exact reachable `(states, edges)` for finite systems.
- `state_vars` — ranked, deduplicated interface variables; defines node labels and
  the layout/label axis order.
- `categorical_vars` — top categorical var drives the node-fill color channel.
- `numeric_vars` (`kind ∈ {int, real}`) — the axes that get grid-sampled in the
  numeric branch.
- `enum_variants[name]` — default seed value for enum vars in the numeric branch.

No transition relation is ever hardcoded; the graph is built **entirely** by solving
transition queries.

---

## 4. Algorithm

### 4.1 Graph construction — branch on program shape

Let `key(state) = (state[v.name] for v in state_vars)` be the canonical node key.

**Branch A — discrete (`is_discrete()` true).** Build the *exact* graph:
1. `(states, edges) ← reachable()`.
2. Add one node per `key(state)`.
3. For each edge `(i, j)`, add `key(states[i]) → key(states[j])`. Keep self-loops
   (`i == j`) — they mark fixed points and make the SCC recurrent.

**Branch B — mixed with at least one numeric var.** First *attempt* the exact graph:
1. `(states, edges) ← reachable(limit=4000)`.
2. If `1 < |states| < 2000`, build `G` exactly as in Branch A (the system is
   finite/terminating, e.g. a vending machine). Otherwise fall through to Branch C.

**Branch C — pure numeric, or mixed that did not terminate.** Build an *approximate*
flow graph by grid-sampling + quantization (see 4.2).

**Branch D — no numeric vars and not flagged discrete.** Fall back to Branch A's
exact construction on whatever `reachable()` returns.

### 4.2 Numeric grid-sampling (approximate flow graph)

The recurrence skeleton of a continuous-state system (e.g. a limit cycle) is
recovered by quantizing state space into a coarse lattice and stepping seeds forward:

1. **Quantization.** Choose `cells` cells per numeric axis (coarse on purpose — the
   Morse graph wants the limit set to *collapse* into one nontrivial SCC, not a fine
   flow field). With sampling half-width `span`, cell size is
   `cell = 2·span / cells`. Quantize a state by mapping each numeric component to
   `round(value / cell)` and keeping non-numeric components verbatim:
   ```
   quant(state) = tuple(
       round(state[v]/cell)  if v numeric  else  state[v]
       for v in state_vars
   )
   ```
2. **Seed grid.** Build a 1-D axis of `n` points evenly spaced on `[-span, span]`:
   `axis[i] = -span + 2·span·i/(n-1)`. Take the Cartesian product of this axis over
   **every** numeric var to get the seed coordinates. Seed all non-numeric vars from
   `initial_state()` (or a default: `false` for bool, `enum_variants[v][0]` for
   enum) so each seed is a complete state.
3. **Step + accumulate edges.** For each seed `s`:
   - `nxt ← successor(s)`; skip if unsat.
   - Add nodes `quant(s)`, `quant(nxt)` and edge `quant(s) → quant(nxt)`.
   - **Follow the chain** up to `chain_steps` more steps:
     `cur ← nxt`; repeat `cur ← successor(cur)`, adding the quantized edge each time,
     until unsat or the step budget is exhausted. Following the chain is what lets a
     trajectory *close* into a cycle in the quantized lattice — without it, a single
     forward step rarely produces a non-trivial SCC.
4. **Centroids for labels.** For each cell, remember the first concrete numeric
   coordinate that landed in it (`setdefault`), to label the lattice node with a
   representative point `(x, y, …)`.

The recurrent SCCs of this quantized flow graph trace the attracting set (limit
cycle = one nontrivial attracting SCC; spiral sink = a small attracting cell cluster).

### 4.3 Condensation + classification

1. Compute SCCs of `G` (any linear-time algorithm: **Tarjan** or **Kosaraju**).
2. Form the condensation DAG `C`; record each node's member set.
3. For each condensation node, compute `size`, `is_cycle`, in/out-degree in `C`, and
   `role` per the rules in §2.
4. **Tint value (optional).** If a categorical tint var exists (§5), compute each
   SCC's *dominant* value of that var: the mode of `member[tint_index]` over the
   SCC's members. This becomes the node's fill color.

### 4.4 Skeleton simplification (legibility for large graphs)

When `C` has more than `simplify_threshold` nodes, collapse the transient cloud so
the recurrence skeleton stays readable:

1. **Keep** every node that is a cycle SCC, or whose role is repeller / attractor /
   isolated (the boundary of the flow). **Merge** all remaining singleton transients.
2. If nothing would be merged, return `C` unchanged.
3. Otherwise create `C'`: copy all kept nodes; add one **summary node**
   (`"N transient cells (flow-through)"`). Remap every original `C`-edge through
   `keep ? self : summary`, dropping self-edges on the summary.
4. **Recompute roles** on the kept nodes using their *new* degrees in `C'` (a node
   that lost its only out-edge to the merged cloud may now correctly read as an
   attractor; a recurrent SCC with no outflow is a real attractor).

### 4.5 Layout + rendering geometry

A renderer needs a DAG layout and boxed nodes:

1. **Layout.** Use a layered/hierarchical DAG layout (e.g. **Sugiyama** layered
   layout — what Graphviz `dot` implements: rank assignment by longest path, crossing
   minimization, top-down ordering) so the flow reads top (sources) → bottom (sinks).
   Fall back to a force-directed (spring) layout if hierarchical layout is
   unavailable.
2. **Normalize** node positions into the unit square `[0,1]²`.
3. **Edges** drawn as arrows `u → v`, slightly curved, drawn under the nodes.
4. **Nodes** drawn as rounded boxes with:
   - **border color = role** (attractor green, repeller red, transient blue,
     isolated purple),
   - **border weight = is_cycle** (thick double border for recurrent SCCs, thin for
     single non-recurrent states),
   - **fill = dominant categorical value** (light tint), or white when no tint var,
   - **text** = node label (see §4.6).
5. **Legends.** One for role→border-color (+ "cycle SCC = thick border"); a second,
   only if tinting, mapping fill→dominant categorical value.

### 4.6 Node labels

- **Single-state SCC** → label by ranked vars: the top-ranked var spelled out
  (`name=val`), remaining vars as a compact strip; for bool vars surface only the
  `True` flags (reads as "what's set"). Strip dotted prefixes and `has_`/`is_`
  prefixes from leaf names for terseness.
- **Cycle SCC** → `"cycle ×N"` plus the labels of up to 2 member states, `…` if more.
- **Numeric lattice cell** → the representative centroid `(x, y, …)` rounded to ints.
- **Summary node** → `"N transient cells (flow-through)"`.

Value formatting: `True→"T"`, `False→"F"`, floats rounded to integer, else `str`.

---

## 5. Variable → channel mapping

The Morse graph is a **graph**, not a Cartesian plot — most variables are encoded
*structurally* (in the node identity / SCC membership), not on positional axes.

| Channel | Variable | Reasoning |
|---|---|---|
| **graph position (layout)** | derived: condensation rank / flow order | Position encodes the *partial order of flow*, not a state var — quantitative effectiveness applied to the flow itself. |
| **node fill color** | top-ranked **categorical** var (`categorical_vars[0]`) | Color is most effective for categorical data; the SCC's dominant value tints the node so the mode is read at a glance. |
| **node border color** | derived **role** (categorical: attractor/repeller/transient/isolated) | A second categorical channel, orthogonal to fill, carried by hue. |
| **node border weight** | derived **is_cycle** (binary) | A binary distinction maps cleanly to a thickness/redundant-encoding channel. |
| **node label / identity** | all `state_vars` (ranked) | The full state lives in the node key; the top ranked var gets named prominence in the label. |

There is **no facet variable** — the entire recurrence structure is one connected
DAG and faceting would sever the flow edges that are the whole point.

The categorical→color, derived-role→color, binary→weight assignment follows
type-effectiveness ordering (Mackinlay 1986): position for the (derived) quantitative
order, hue for the nominal role/category, size/weight for the binary recurrence flag.

---

## 6. Degradation & edge cases

- **Pure numeric (no categorical var):** no tint; nodes stay white, read by
  role/border alone. The graph is the grid-sampled approximation (Branch C).
- **Mixed but finite:** exact graph via `reachable()` (Branch B accepts it if
  `1 < |states| < 2000`); behaves like the discrete case with numeric components
  carried in the node key.
- **Mixed but non-terminating:** falls through to grid sampling (Branch C); numeric
  axes quantized, categorical/bool axes seeded and carried verbatim.
- **Empty / single-state reachable set:** condensation has 0–1 nodes; render a
  placeholder ("empty graph") or a single classified node (an isolated fixed point).
- **Huge condensation (`> simplify_threshold` nodes):** apply §4.4 skeleton
  simplification — keep recurrent + boundary SCCs, merge the transient cloud.
- **Layout backend missing:** fall back from hierarchical (Sugiyama/`dot`) to a
  force-directed spring layout with a fixed seed for determinism.
- **`reachable()` raises or times out in the mixed branch:** treat as empty and fall
  to grid sampling.

---

## 7. Parameters

| Parameter | Default | Meaning |
|---|---|---|
| `reachable` BFS cap | 5000 | Max states explored for the exact graph. |
| mixed-branch reachable limit | 4000 | Cap when trying exact graph on a mixed system. |
| mixed-branch accept range | `1 < |states| < 2000` | Below this upper bound, accept the exact graph; else grid-sample. |
| `span` (sampling half-width) | 3200 | Numeric seeds drawn from `[-span, span]` per axis. |
| `n` (seeds per axis) | 13 | Grid resolution; total seeds = `nᵈ` over `d` numeric vars. |
| `cells` (quantization) | 8 | Cells per numeric axis; `cell = 2·span/cells`. Coarse, so the limit set collapses into few SCCs. |
| `chain_steps` | 40 | Extra forward steps followed per seed so trajectories close into cycles. |
| `simplify_threshold` | 24 | Above this many condensation nodes, run skeleton simplification. |

Note the numeric branch is **exponential in the number of numeric vars** (`nᵈ`
seeds); it is designed for `d = 1–2`. For higher `d`, reduce `n` or project to the
top two ranked numeric vars.

---

## 8. References

- **Conley index theory / Morse decomposition** — C. Conley, *Isolated Invariant
  Sets and the Morse Index* (CBMS 38, 1978). The recurrence-set-plus-partial-order
  structure the Morse graph is the finite analogue of.
- **Computational Conley–Morse graphs** — Arai, Kalies, Kokubu, Mischaikow, Oka,
  Pilarczyk, *A Database Schema for the Analysis of Global Dynamics of Multiparameter
  Systems* (SIAM J. Appl. Dyn. Syst., 2009); Bush et al., *Combinatorial-topological
  framework for the analysis of global dynamics* (Chaos, 2012). These define exactly
  the "outer-approximate the map on a cell grid, take SCCs, condense" pipeline used in
  Branch C.
- **Tarjan, R.** *Depth-first search and linear graph algorithms* (SIAM J. Comput.,
  1972) — strongly-connected components and condensation in linear time.
- **Sugiyama, K., Tagawa, S., Toda, M.** *Methods for visual understanding of
  hierarchical system structures* (IEEE SMC, 1981) — the layered DAG layout used to
  render flow top-down (Graphviz `dot`).
- **Mackinlay, J.** *Automating the design of graphical presentations of relational
  information* (ACM TOG, 1986) — the type-effectiveness ranking (position >
  color/hue) behind the channel mapping in §5.
