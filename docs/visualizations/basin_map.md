# basin_map — Basins of Attraction

A reimplementable specification of the basin-of-attraction visualization for an
Evident program. Language- and toolkit-agnostic: it assumes only an SMT solver
exposed through the shared transition queries and a 2-D drawing surface. All
shared primitives (`reachable`, `successor`, `initial_state`, `is_discrete`,
`state_vars`, `numeric_vars`, `categorical_vars`, `assign_channels`,
`facet_var`, `enum_variants`, `label`, `_key`, variable ranking/dedup/mRMR,
channel mapping, facet guard) are defined in
`docs/visualizations/00-core-machinery.md` — reference that file; this document
does not re-derive them.

---

## 1. What it shows

An Evident program is a (possibly nondeterministic) discrete dynamical system:
a transition relation `f` over a finite vector of state variables. Iterating `f`
from a start state, the orbit eventually settles into a **terminal set** — a
fixed point, a limit cycle, or a terminal strongly-connected component (an SCC
with no outgoing edges in the quotient graph). The **basin** of a terminal set
is the set of all start states whose forward orbit flows into it.

basin_map colors a 2-D projection of state space by *which terminal set* each
start state ends up in. It answers: **"From where does the system go where?"** —
the global, long-run partition of state space by destiny.

Use it when you care about multistability / attractor structure: how many
attractors exist, how the plane divides between them, where the basin boundaries
(separatrices) lie. It applies to all three program shapes, with different
machinery per shape:

| Shape | Detection | Method |
|---|---|---|
| **discrete** (all bool/enum/string) | `is_discrete()` true | exact reachable graph → SCC condensation → terminal basins |
| **numeric** (all int/real) | `is_discrete()` false, every state var numeric | seed grid → iterate to convergence → cluster attractors |
| **mixed** (some numeric, some categorical) | `is_discrete()` false, not all numeric | same as numeric; categorical axes ordinalized, non-axis categoricals held at baseline |

---

## 2. The object

Two distinct mathematical objects share a presentation (a colored 2-D scatter
where color = basin identity):

**Discrete object.** Given the reachable transition graph `G = (V, E)`, form the
**condensation** `G/≈` by contracting each strongly-connected component to a
node. `G/≈` is a DAG. A component is **terminal** iff it has out-degree 0 in
`G/≈`. Every node `v` is colored by the terminal SCC its component can reach
(its *basin*); reachable nodes whose component reaches several terminals are
assigned a deterministic *dominant* one. The object is the reachable-state
scatter on two chosen axes, each point colored by basin, with the transition
edges drawn as faint connectors.

**Numeric/mixed object.** The classic basin-of-attraction image: a regular grid
of seed states over two state axes is iterated forward to convergence; each seed
cell is colored by the **attractor region** its orbit lands in. Attractor regions
are obtained by clustering phase-invariant signatures of the observed attractors,
so the image is a discrete approximation of the continuous basin partition with
attractor locations overlaid as markers.

---

## 3. Inputs (core-machinery primitives consumed)

- `is_discrete()` — selects the discrete vs numeric/mixed branch.
- **Discrete branch:** `reachable()` → `(states, edges)`; `label(state)`,
  `enum_variants`.
- **Numeric/mixed branch:** `initial_state()`, `successor(state)` (one map
  step; `None` ⇒ dead-end/fixed point), `reachable(limit)` (used only to
  *bound* numeric axes), `_key(state)` (canonical hashable state key).
- **Both:** `state_vars` (ranked, deduped), `numeric_vars`,
  `assign_channels(["x","y"])`, `facet_var()`, `enum_variants`.

basin_map never re-solves a successor it has already computed: it memoizes
`successor` results by `_key` and short-circuits orbits that merge onto
already-resolved territory (§4.2).

---

## 4. Algorithm

### 4.1 Discrete branch — terminal-SCC basins

1. **Reachable graph.** Call `reachable()` → `(states, edges)`, with states
   indexed `0..n-1`. If empty (no initial state), emit a placeholder. Build a
   simple directed adjacency list, dropping self-loops and duplicate edges.
   Keep the deduped edge set `E`.

2. **SCC decomposition.** Run **Tarjan's algorithm** (single DFS, `O(V+E)`),
   iteratively to avoid recursion-depth limits. This yields the list of SCCs and
   a map `scc_of[v]` from node to component id.

3. **Condensation DAG.** For each edge `(a,b) ∈ E` with
   `scc_of[a] ≠ scc_of[b]`, add edge `scc_of[a] → scc_of[b]` to the quotient
   graph `cadj`. By construction `cadj` is acyclic.

4. **Terminal components.** Component `s` is **terminal** iff
   `out-degree(s) = 0` in `cadj`. Collect `term_ids` and assign each a
   contiguous `term_index`.

5. **Reachable-terminal sets.** For every component `s`, compute
   `reach_term[s]` = the set of terminal components reachable from `s` in
   `cadj`. Compute by iterating `reach_term[s] = {s if terminal} ∪ ⋃_{t∈cadj[s]} reach_term[t]`
   to a fixpoint. (A reverse-topological sweep does it in one pass; fixpoint
   iteration is the robust equivalent.)

6. **Basin color of a node.** For node `v`, let `rt = reach_term[scc_of[v]]`.
   If empty, color = "no terminal" (a pathological case). Otherwise pick the
   **dominant** terminal: `argmin_{s ∈ rt} term_index[s]` — a deterministic
   tie-break so a multi-destination node gets one stable color. The basin color
   index is `term_index[dom]`.

7. **Project & draw.** Choose axes (§5). Map each state to plot coords via the
   **ordinal encoding** `ord(v, value)`:
   `int/real → value; bool → {0,1}; enum → index in enum_variants; string → 0`.
   Add small uniform jitter (±0.11 in each axis, seeded RNG) to separate
   coincident projected points. Draw the deduped edges `E` as faint gray
   connectors between the (jittered) endpoint coords, then scatter the nodes
   colored by basin index. The legend maps each basin color to a representative
   terminal: `→ label(rep_state) (fixed pt | cycle)`, where the terminal is a
   *cycle* iff its SCC has > 1 node, else a *fixed point*.

8. **Optional facet** (§5): if a suitable facet variable exists, render one
   panel per facet value (states partitioned by that variable's value), sharing
   axes and a single global legend so a basin color means the same thing in
   every panel.

### 4.2 Numeric / mixed branch — grid + iterate + cluster

1. **Axes & facet.** Choose `ax_x` and optional `ax_y` (§5); choose an optional
   facet variable.

2. **Per-axis grid & bounds.** For a numeric axis, derive `(lo, hi)`:
   - **Sampled bounds:** call `reachable(limit≈2000)`; for each numeric var, if
     the observed values span ≥ 2 distinct values AND `max−min ≤ 64`, trust
     `(min, max)` as a tight domain. (Pads by ±15%, clamps step count to the
     integer span for int axes.) This gives small-domain axes (e.g. a balance
     `0..3`) a tight grid.
   - **Heuristic fallback:** otherwise use a wide symmetric window
     (default `[−3200, 3200]`) suited to large continuous samples.
   Numeric axes sample `~28` grid points (or the integer span if smaller); a
   bool axis samples `{false, true}`; an enum axis samples all its variants; a
   missing `ax_y` collapses to the single value `[0]`.

3. **Seed state construction.** For each grid cell `(xv, yv)`: start from a
   **baseline** state — `initial_state()` if available, else neutral defaults
   (`0` / `false` / first enum variant / `""`) — override any fixed facet var,
   then set `ax_x = xv` (and `ax_y = yv`), rounding to int for int axes.

4. **Iterate each seed to its attractor.** Follow the successor chain to
   convergence, returning a **phase-invariant attractor signature** (§4.3).
   Two memos keep a full grid tractable:
   - `cache`: `_key(state) → successor` — never re-solves the SMT successor.
   - `resolved`: `_key(state) → signature` — once a chain settles, every state
     along it is tagged with the attractor it reached; a later chain that
     touches any tagged state short-circuits instantly.
   Termination cases within an orbit: (a) the current key is already `resolved`
   → return its signature; (b) the key repeats within this orbit → a cycle is
   closed, signature from the cycle slice; (c) `successor` returns `None`
   (dead-end / fixed point) → signature from the singleton; (d) `max_steps`
   (default 4000) exhausted → best-effort signature from the orbit tail.

5. **Limit-cycle probes.** When `ax_x` is a *large-range* numeric axis (the
   heuristic-bounds case, not a tight sampled domain), add a handful of
   off-origin probe seeds in axis-value space (defaults: `(±2800,0)`,
   `(±400,0)`, `(0,±2700)`, `(1500,1500)`). Rationale: autonomous systems whose
   `initial_state` sits *at* a fixed point (origin) can have a surrounding limit
   cycle that a pure interior grid misses if every seed collapses inward; the
   probes guarantee the cycle attractor is sampled so it earns a color/label.
   Probes feed the clustering but are not drawn as markers. (For a categorical
   `ax_y`, the probe x-magnitudes are swept across every grid y value.)

6. **Cluster signatures → basins.** Greedy single-pass clustering over the
   signature vectors with an L1 distance and absolute tolerance (default
   `tol = 400`): each signature joins the nearest existing center within `tol`
   (online mean update of that center) or starts a new center. The number of
   centers = number of distinct attractor basins; each seed's basin label is its
   center id. Clustering is done **once across all panels** (faceted runs
   cluster the concatenation of every panel's signatures) so a basin id means the
   same attractor in every panel.

7. **Draw.** For each basin id, scatter its seed cells (square markers; small
   for a continuous grid, large for a categorical/coarse grid) in its palette
   color. Map cluster-center centroids back onto the numeric axes and overlay
   them as star markers (the attractor locations). Decorate enum/bool axes with
   categorical tick labels. Legend entries are human-readable region
   descriptions (§4.4).

### 4.3 Phase-invariant attractor signature

A signature must be identical for two orbits landing on the same attractor,
**independent of where on a cycle each orbit happened to stop** — otherwise every
phase of a limit cycle reads as its own basin. Given a cycle (list of states):

- **Numeric vars** (in `state_vars` order): the per-variable **centroid**
  `mean_t ord(v, state_t[v])` (one coord each), followed by the **mean orbit
  radius** `mean_t ‖ord(state_t) − centroid‖₂` over the numeric coords. Both are
  invariant to cyclic phase. A fixed point has radius ≈ 0; a limit cycle has
  radius > 0.
- **Discrete vars** (enum/bool/string): the **set of visited ordinal values**
  along the cycle, encoded as a single base-1000 positional number (one coord
  per discrete var). The visited-set genuinely distinguishes discrete attractors
  (which mode-cycle the orbit settles into).
- A final coord = cycle length.

`_sig_dist(a,b) = Σ_i |a_i − b_i|` over the shared prefix (L1).

### 4.4 Region description (legend text)

From a cluster center, reconstruct a label using the fixed signature layout
`[numeric centroids…, orbit radius (if any numeric), discrete set-codes…, cycle
length]`: print `name≈centroid` per numeric var, `r≈radius`, and decode each
discrete set-code (base-1000 digits → ordinal values → enum/bool names). Classify
as **cycle** iff cycle length > 1.5 or radius > 150, else **fixed**.

---

## 5. Variable → channel mapping

Two **position** channels (x, y) carry the projection; **color** is reserved for
basin identity (it is the output of the algorithm, not an input variable);
**facet** optionally carries a third, near-constant categorical variable.

- **Axes (x, y).** basin_map wants quantitative position axes because a
  continuous seed grid is only meaningful on an ordered domain. So it prefers
  `numeric_vars` (top-ranked, best-decoding quantitatives) for both slots, in
  rank order, deduped. If fewer than two numeric vars exist, top up the remaining
  slot(s) from `assign_channels(["x","y"])` — which may supply a categorical,
  ordinalized onto position. Type-effectiveness: quantitative → position is the
  most effective encoding; this is why numeric beats categorical for an axis.

- **Color = basin.** Categorical-by-construction (a small set of terminal
  ids/attractor clusters), so it maps to a qualitative palette — exactly the
  hue-channel-for-categories rule. Color is *not* assigned from a state variable.

- **Facet.** Chosen by the shared **facet guard** `facet_var()`: a
  low-cardinality categorical that stays ~constant *within a run* (default
  thresholds: cardinality ≤ 6, change-rate ≤ 0.25). It must not already be an
  axis. Faceting by a variable that *changes along the trajectory* (e.g. a mode
  that cycles) would split one attractor's dynamics across panels — the guard
  rejects those. Each facet value becomes a small-multiple panel; numeric/mixed
  panels hold the facet var fixed during seeding, discrete panels partition the
  reachable states.

---

## 6. Degradation & edge cases

- **No state variables / no axes:** placeholder image ("no state variables to
  project").
- **Empty reachable set** (no initial state, discrete branch): placeholder.
- **Single axis** (only one suitable variable): render a 1-D basin strip — `ax_y`
  absent, all y-coords 0, x carries the projection.
- **Categorical axis** (mixed): ordinalized via `ord(v,·)` (enum → variant
  index, bool → 0/1); enum/bool axis ticks are relabeled with the variant names.
  Non-axis categoricals are held at their baseline value during seeding.
- **`successor` returns `None`:** treated as a fixed point (dead-end), signature
  from the singleton state.
- **Non-convergent orbit** (chaotic / `max_steps` exceeded): best-effort
  signature from the orbit tail; it still clusters, just less reliably.
- **Attractor-at-origin systems** (initial state is a fixed point with a
  surrounding cycle): the off-origin probe seeds (§4.2 step 5) recover the cycle
  basin that a pure interior grid would miss.
- **Degenerate numeric sample** (every reachable state at one point):
  `_numeric_bounds` returns nothing for that var, so the wide heuristic window is
  used instead of a zero-width grid.
- **Mixed-destiny nodes** (discrete): a node whose component reaches several
  terminals is colored by the deterministic dominant terminal (smallest
  `term_index`) rather than drawn multi-colored — a simplification noted in the
  implementation.

---

## 7. Parameters (with defaults)

| Parameter | Default | Role |
|---|---|---|
| numeric grid resolution per axis | 28 | seed-grid samples along a continuous axis (clamped to integer span if smaller) |
| categorical grid resolution | all variants / `{F,T}` | enum/bool axis sampling |
| `max_steps` (orbit iteration) | 4000 | cap on successor steps before best-effort signature |
| reachable axis-bound sample limit | 2000 | states sampled to derive tight numeric bounds |
| tight-bound span threshold | `max−min ≤ 64` | when to trust sampled bounds vs heuristic window |
| heuristic numeric window | `[−3200, 3200]` | fallback axis domain for large continuous systems |
| bound padding | 15% of span | display margin around a tight domain |
| cluster tolerance `tol` | 400 (L1) | merge radius for attractor signatures |
| jitter amplitude (discrete) | ±0.11 per axis | separate coincident projected states |
| limit-cycle probe seeds | `(±2800,0),(±400,0),(0,±2700),(1500,1500)` | guarantee cycle sampling on wide numeric axes |
| facet max cardinality / max change-rate | 6 / 0.25 | facet guard thresholds (shared) |

---

## 8. References

- **Strongly-connected components & condensation:** R. Tarjan, "Depth-first
  search and linear graph algorithms," *SIAM J. Computing* 1(2), 1972. The
  terminal-SCC characterization of recurrent/absorbing sets is standard in the
  theory of finite Markov chains and finite automata (closed communicating
  classes / sink components).
- **Basins of attraction, attractors, separatrices:** Guckenheimer & Holmes,
  *Nonlinear Oscillations, Dynamical Systems, and Bifurcations of Vector Fields*
  (1983); Strogatz, *Nonlinear Dynamics and Chaos* (1994) — fixed points, limit
  cycles, multistability, basin boundaries.
- **Cell-mapping / grid approximation of basins:** C. S. Hsu, *Cell-to-Cell
  Mapping* (1987) — discretizing state space into cells and following the induced
  map to attractors is exactly the seed-grid construction here.
- **Visualization channel effectiveness:** Cleveland & McGill (1984) on
  position vs. other encodings; Mackinlay, "Automating the design of graphical
  presentations" (1986); Munzner, *Visualization Analysis and Design* (2014) —
  quantitative→position, categorical→hue, and small-multiples (Tufte, *Envisioning
  Information*, 1990) for faceting.
- **SMT-based reachability:** the underlying transition queries (bounded model
  checking / block-and-re-solve enumeration) are documented in
  `docs/visualizations/00-core-machinery.md`.
