# Reachability Tree

A language-agnostic algorithm specification. Prerequisite reading:
`docs/visualizations/00-core-machinery.md`, which defines the shared transition
queries (`initial_state`, `successor`, `successors`), variable ranking, channel
assignment, and the discrete/numeric classifier referenced below. This document
specifies only what is unique to the reachability-tree renderer.

## 1. What it shows

The **breadth-first reachability tree** of an Evident program viewed as a
difference equation (transition relation over a finite set of state variables).
It answers: *starting from the initial state, which states can the system reach,
how many steps does the shortest path to each take, and where does it get
stuck?*

It is the tree-shaped projection of the reachable state graph: every reachable
state appears once, positioned at its BFS depth (the length of the shortest path
from the root), connected by exactly one incoming edge — the edge along which it
was first discovered. Back-edges, cross-edges, and self-loops are dropped, so the
drawing is a tree, not the full graph (contrast with the `state_graph` viz, which
keeps every edge).

**When to use it.** Best for **discrete** programs (all interface variables are
bool / enum / string) whose reachable set is finite and modest (tens of states):
protocol/handshake state machines, puzzle solvers, turn-based game logic. For
**numeric** or **mixed** programs the reachable set is generally unbounded, so the
tree is a capped sample showing local branching structure near a seed rather than
a complete reachability proof.

## 2. The object

Formally, let `T ⊆ S × S` be the transition relation (a state `s` may have a set
of successors `T(s) = { s' : (s, s') ∈ T }`, possibly empty, singleton, or
many-valued for nondeterministic programs). Let `r ∈ S` be the seed (root).

Run BFS from `r`. Define for each discovered state `v` its **BFS depth**
`d(v) = ` shortest-path distance from `r` in the graph `(S, T)`. The reachability
tree is `(V, E)` where:

- `V` = the set of states reached before the node/depth caps fire,
- `E` = the **first-discovery (tree) edges**: for each `v ≠ r`, the single edge
  `(u, v)` where `u` is the state from whose expansion `v` was first enqueued.

`E` is precisely the BFS spanning tree of the reachable subgraph. A node `v` is
**absorbing** (a goal / sink, drawn with a red ring) iff `T(v) = ∅` or
`T(v) = {v}` (only self-loops) — i.e. the dynamics cannot leave `v`.

This is the standard BFS tree / shortest-path tree of a directed graph defined
implicitly by a successor oracle.

## 3. Inputs

All dynamics come from the shared transition oracle (core-machinery); nothing is
hardcoded per program.

- `initial_state()` — candidate root; the first-tick state, or `None`.
- `successor(state)` — used once during seed selection to test whether the
  initial state is a fixed point.
- `successors(state, limit=32)` — the BFS expansion oracle: ALL distinct next
  states (set-valued image / fan), via block-and-re-solve.
- `state_vars` — ranked, deduplicated interface variables (for the numeric seed
  fallback and for kind classification).
- `categorical_vars` — categorical (enum/bool/string) interface variables, top
  one drives the color channel.
- `enum_variants[name]` — the declared domain of an enum variable (palette order;
  numeric-seed bool/enum defaults).
- `is_discrete()` — classifier; controls the "reachable set unbounded" caption.
- `label(state)` — short node label (tuple of interface-variable values).

## 4. Algorithm

### 4.1 Seed selection (`_pick_seed`)

The tree must start from a state that *moves*, or it degenerates to a single
self-looping node.

1. Let `init = initial_state()`. If `init` exists, compute `succ = successor(init)`.
   If `succ` is `None` (no successor) **or** `succ ≠ init` (genuinely moves), use
   `init` as the seed; source tag = `"initial_state"`. Done.
2. Otherwise (`init` missing, or `init` is a fixed point `successor(init) = init`):
   - Let `numeric = { v : v.kind ∈ {int, real} }` over `state_vars`.
   - If `numeric` is nonempty, synthesize a **grid seed** off the fixed point:
     for each state var `v`, assign
     - numeric `v`: `2800` if the variable name ends in `.x` or contains `x`
       (case-insensitive), else `0` — a heuristic "point on a limit cycle, off the
       origin" (tuned for van-der-pol-class oscillators);
     - bool `v`: `false`; enum `v`: `enum_variants[v][0]`; string `v`: `""`.
   - If after this every numeric axis is still `0`, force the lexicographically
     first numeric var to `2800` (guarantee at least one nonzero axis).
   - Source tag = `"grid seed"`.
   - If there are no numeric vars and no usable init, return `None` (placeholder
     figure, §6).

### 4.2 BFS tree construction (`build_tree`)

Caps: `MAX_NODES = 60`, `MAX_DEPTH = 8`. States are keyed by a canonical,
order-independent tuple of `(name, value)` pairs (sort variable names so equal
states collide regardless of dict ordering).

1. Initialize: root key `r = key(seed)`; `states = {r: seed}`; `depth = {r: 0}`;
   add node `r`; `frontier = [r]` (a FIFO queue); `absorbing = ∅`;
   `truncated = false`.
2. While `frontier` nonempty **and** `|V| < MAX_NODES`:
   1. Dequeue `k` (front of FIFO — preserves BFS / shortest-path order).
   2. If `depth[k] ≥ MAX_DEPTH`, skip expansion (depth cap).
   3. Compute `succs = successors(states[k], limit=32)`.
   4. If `succs` is empty → mark `k` absorbing; continue.
   5. Let `non_self = { s ∈ succs : key(s) ≠ k }`. If empty (only self-loops),
      mark `k` absorbing (but still continue — there may be the self-loop only).
   6. For each `ns ∈ succs`:
      - Let `nk = key(ns)`. If `nk = k`, skip (self-loops are never drawn).
      - If `nk` is **new** (not in `states`):
        - If `|V| ≥ MAX_NODES`, set `truncated = true` and break out of the
          successor loop (node cap).
        - Record `states[nk] = ns`, `depth[nk] = depth[k] + 1`, add node `nk`,
          add the **first-discovery edge** `(k, nk)`, enqueue `nk`.
      - If `nk` already seen → it is a back/cross edge; **omit** it (keeps a tree).
3. Return `(V, E, states, depth, absorbing, r, truncated)`.

Because expansion is strictly FIFO and each node keeps only its first incoming
edge, `depth[v]` equals the true BFS shortest-path distance and `E` is the BFS
spanning tree.

### 4.3 Layout

Lay out the tree top-down so the y-coordinate encodes depth (root at top,
leaves at bottom), children spread horizontally under their parent. Reference
implementation uses graphviz's `dot` hierarchical layout (Sugiyama-style layered
DAG layout: assign nodes to layers by depth, order within a layer to minimize
edge crossings, then assign x-coordinates). Any layered tree layout works. If the
hierarchical layout is unavailable, fall back to a force-directed (spring) layout
with a fixed seed for determinism.

### 4.4 Rendering

- **Edges**: directed arrows parent → child, uniform light-gray.
- **Nodes**: uniform-size discs. Fill = color channel (§5). Border ring:
  green + thick for the root, red + thick for absorbing/goal nodes, thin dark
  gray otherwise.
- **Node labels**: `label(state)` — the interface-variable tuple — at small font.
- **Title/subtitle** report: node count, max depth reached, seed source tag,
  discrete-vs-numeric classification, and a `TRUNCATED at MAX_NODES/MAX_DEPTH`
  flag when caps fired. For numeric programs append a note that the reachable set
  is unbounded and this is a capped sample.

## 5. Variable → channel mapping

This viz has a fixed spatial layout (position encodes tree structure / depth, not
data values), so the only data-driven visual channel is **node color**.

- **Color = top categorical variable.** Take `cat = categorical_vars[0]` (the
  highest-ranked enum/bool/string interface var — see core-machinery ranking).
  Build its domain:
  - enum → `enum_variants[name]` (declared order),
  - bool → `[false, true]`,
  - string/other → values observed across tree nodes, in first-seen order.
  Then fold in any observed value not already in the domain (defensive). Assign
  one hue per domain value from a qualitative palette (`tab10` if ≤ 10 values,
  else `tab20` — categorical/qualitative colormap, not a sequential one). Emit a
  legend `color = <var name>` with one swatch per value. This follows
  Bertin/Mackinlay effectiveness: a categorical attribute is best carried by hue.
- **Fallback — depth gradient.** If there is no categorical variable (pure-numeric
  systems), color each node by a sequential colormap (Blues) interpolated on
  `t = depth(v) / max_depth` over `[0.35, 0.85]`. This is a legitimate coarse
  *quantitative* use of color (sequential colormap for an ordered quantity).
- **Root/absorbing** are encoded redundantly via border ring color, independent
  of the fill channel, so they remain visible under either color scheme.

Position (x, y) is **not** a free channel here — y is depth and x is layout
spacing; neither maps to a user variable.

## 6. Degradation & edge cases

- **No seed at all** (`initial_state` is `None` and no numeric vars to grid-seed)
  → render a placeholder figure stating "no initial state and no numeric seed";
  no tree.
- **Initial state is a fixed point** → grid-seed off it (§4.1) so the tree shows
  real dynamics instead of one self-looping node.
- **Discrete program** → the tree is (up to the caps) the exact, complete BFS
  reachability tree; typically small and fully explored.
- **Numeric / mixed program** → reachable set generally infinite; the caps make
  it a finite local sample. Subtitle flags this and reports truncation.
- **Truncation** → when `MAX_NODES` or `MAX_DEPTH` fires, the partial tree is
  still valid (every drawn edge is a true first-discovery edge; every drawn depth
  is a true shortest-path distance); only completeness is lost.
- **Empty tree** (`|V| = 0`, should not happen given a seed) → "empty reachable
  tree" message.
- **Nondeterminism** is handled natively: `successors` returns the full fan, so a
  state with multiple distinct next states branches into multiple children.

## 7. Parameters

| Parameter | Default | Meaning |
|---|---|---|
| `MAX_NODES` | 60 | Hard cap on tree size; stops BFS, sets truncation flag. |
| `MAX_DEPTH` | 8 | Nodes at this depth are not expanded (BFS frontier cutoff). |
| `successors` limit | 32 | Max successors enumerated per state during expansion. |
| grid-seed numeric value | 2800 | Off-origin seed magnitude for numeric axes (limit-cycle-scale heuristic). |
| palette switch | 10 | ≤ 10 categorical values → `tab10`, else `tab20`. |
| depth-gradient range | `[0.35, 0.85]` | Sequential-colormap interpolation band for the no-categorical fallback. |

The caps trade completeness for legibility and termination; raise them for fuller
discrete graphs, lower them for dense numeric fans.

## 8. References

- **Breadth-first search & BFS / shortest-path spanning tree** — Cormen, Leiserson,
  Rivest, Stein, *Introduction to Algorithms*, ch. 22 (BFS computes
  shortest-path distances and a predecessor/BFS tree in unweighted graphs).
- **Reachability analysis / state-space exploration** — Clarke, Grumberg, Peled,
  *Model Checking* (explicit-state reachable-set construction from a transition
  relation); Holzmann, *The SPIN Model Checker*.
- **Layered (hierarchical) graph drawing** — Sugiyama, Tagawa, Toda, "Methods for
  Visual Understanding of Hierarchical System Structures" (IEEE SMC, 1981); the
  basis of graphviz `dot`. Gansner et al., "A Technique for Drawing Directed
  Graphs" (IEEE TSE, 1993).
- **Visual encoding effectiveness** — Bertin, *Sémiologie Graphique* (1967);
  Mackinlay, "Automating the Design of Graphical Presentations" (ACM TOG, 1986)
  — categorical → hue, ordered quantity → sequential value/lightness.
- **Qualitative vs. sequential colormaps** — Brewer, *ColorBrewer* design
  principles (qualitative palette for nominal categories, sequential for ordered
  magnitudes).
