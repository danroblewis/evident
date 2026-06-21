# Occupancy Heatmap

A density heatmap over two state axes showing **where the system spends its
time** — the empirical occupation measure of the dynamical system on a 2-D
projection of state space. Bright cells are the attractor / dwell region; dark
cells are visited rarely or never.

> Prerequisite: read [`00-core-machinery.md`](00-core-machinery.md) for the
> shared transition queries (`initial_state`, `successor`, `successors`,
> `trajectory`, `reachable`), variable ranking/dedup (`state_vars`,
> `numeric_vars`, `categorical_vars`, `enum_variants`, `is_discrete`), and the
> channel/facet helpers (`assign_channels`, `facet_var`). This doc specifies
> only what is unique to the occupancy heatmap; those primitives are referenced
> by name and not re-derived.

---

## 1. What it shows

The question: **of all the states the system visits, which regions does it
occupy most?** This is the *invariant measure* (or its finite-sample
approximation, the occupation measure) of the map — the long-run fraction of
time spent near each point. Peaks reveal fixed points and limit cycles
(attractors); spread reveals transient meandering or chaotic filling.

Use it when you want a single picture of the attractor's *location and shape*,
without tracing individual orbits. Complements per-orbit views (trajectory,
cobweb, phase portrait), which show *paths*; this shows *residence*.

Applicable to all program shapes:
- **Numeric** (real/int state): the 2-D histogram lands on metric axes; the hot
  blob is the attractor in continuous-ish state space.
- **Discrete** (enum/bool state): occupancy over the finite reachable graph;
  cells are state combinations, brightness is visit traffic.
- **Mixed**: any axis that is non-numeric is ordinalized (below) so the
  histogram is still well-defined.

---

## 2. The object

Given a 2-D projection π(s) = (a₀(s), a₁(s)) of the state s onto two chosen
axis variables, the object is the **2-D histogram of visited states under π**,
displayed as a heatmap with a logarithmic intensity transform:

    H[i,j] = #{ visited states s : π(s) falls in bin (i,j) }
    display intensity  =  log(1 + H[i,j])

The log compresses the dynamic range so a heavily-dwelt fixed point does not
saturate the entire field to a single bright pixel while every transient cell
reads as zero. The bin grid is metric for numeric axes and exactly one cell per
discrete value for enum/bool axes.

Optionally faceted: when a low-cardinality configuration variable c exists that
is *not* an axis, the object becomes **small multiples** — one heatmap panel per
value of c, all sharing one color scale, so the third dimension is shown
honestly rather than collapsed.

---

## 3. Inputs

From the model `m` (all defined in core-machinery):

| Primitive | Use here |
|---|---|
| `m.is_discrete()` | selects the collection path (reachable-graph vs. grid-seeded trajectories) |
| `m.numeric_vars`, `m.state_vars` | axis selection (prefer two numeric) |
| `m.assign_channels(["x","y"])` | axis fallback when < 2 numeric vars |
| `m.facet_var()` | a stable low-cardinality config var for small multiples (or None) |
| `m.enum_variants[name]` | ordinal encoding + tick labels for enum axes |
| `m.initial_state()` | a guaranteed-on-trajectory seed (numeric path) |
| `m.trajectory(start, steps)` | dwell sampling along one chain (numeric path) |
| `m.reachable()` | the exact visited set (discrete path) |
| `m.successors(state)` | random-walk dwell traffic over the reachable graph |

Note: `successor`/`trajectory` accept **arbitrary pinned points**, not only
reachable states — this is what lets the numeric path seed a grid over the whole
box rather than only the BFS-reachable set.

---

## 4. Algorithm

### 4.1 Axis selection — `pick_axes(exclude)`

1. Let `numeric = [v ∈ m.numeric_vars : v ∉ exclude]`.
2. **If `|numeric| ≥ 2`**: return `(numeric[0], numeric[1])` — the two
   top-ranked quantitative vars. Histograms are only meaningful on metric axes,
   so numeric is strongly preferred.
3. **Else (mixed/discrete)**: call `m.assign_channels(["x","y"])`; take its `x`
   then `y` assignments (skipping anything in `exclude` or already chosen).
4. If still fewer than 2, top up from `m.state_vars` in ranked order.
5. Return the first two chosen (or `(v, None)` / `(None, None)` if 0/1 exist).

### 4.2 Facet selection (before final axis selection)

1. Tentatively pick axes `(a0_pre, a1_pre)` with no exclusions.
2. **Only on the discrete path** (`m.is_discrete()`), call `m.facet_var()`.
   This returns a categorical var that is **low-cardinality** (≤ ~5–6 values)
   **and approximately constant within a run** (a config/regime set once). A var
   that changes along the trajectory must NOT be a facet — it would split one
   orbit across panels and destroy the dynamics; `facet_var()` already excludes
   those via a change-rate test.
3. If the chosen facet coincides with a tentative axis, drop it.
4. Re-run `pick_axes(exclude = {facet})` so the facet variable is reserved for
   panels, not spent as an axis.

### 4.3 Ordinal projection — `ordinal(var, value) → ℝ`

Every axis/facet value is projected to a real for binning:

- `int`/`real`  → `float(value)`
- `bool`        → `1.0` if true else `0.0`
- `enum`        → index of the variant in `m.enum_variants[name]` (ordinal
  position; the categorical→ordinal encoding)
- `string`      → `(abs(hash(value)) mod 997)` — a stable scatter into a bounded
  range so a string axis at least separates distinct literals (last-resort; a
  string axis carries no metric meaning).

### 4.4 Point collection

The heatmap needs a large bag of visited points whose density reflects
*residence time*, so the same state appearing N times in the bag contributes N
to its cell. Two collection paths:

**Numeric path — `collect_numeric(axes)`** (continuous-attractor sampling):
1. Build a seed set by sampling a coarse **grid** over the two axes:
   `linspace(-span, +span, G)` on each axis (G = 9, `span` = 3200 default),
   giving G² seeds, with all other state vars set to 0. This explores basins
   the reachable BFS could never enumerate.
2. Add a few **explicit off-origin seeds** (e.g. far from a fixed point at the
   origin) so attractors away from 0 are sampled, plus `initial_state()`.
3. For each seed, follow `trajectory(start=seed, steps=S)` (S = 120). **Discard
   the first ≈ T = 10 steps as transient** so the histogram reflects the
   *attractor*, not the approach to it (standard burn-in).
4. Accumulate `ordinal` of each surviving state on both axes into `(xs, ys)`.

**Discrete path — `collect_discrete(axes, facet)`** (finite-graph occupancy):
1. `states, edges = m.reachable()`. **Push every reachable state once** so no
   reachable cell is invisible (baseline occupancy = 1).
2. Then accumulate **dwell traffic**: from `states[0]`, do a seeded random walk
   of K steps (K = 4000), at each step picking a uniform-random element of
   `m.successors(cur)`; push each visited state. This biases the histogram
   toward states the dynamics actually revisits often (the random walk's
   occupation measure on the reachable graph).
3. When faceting, also record the facet value per pushed point.

### 4.5 Binning — `nbins(var)` and 2-D histogram

Per axis, choose bin edges by kind:
- `bool` → edges `[-0.5, 0.5, 1.5]` (exactly 2 cells centered on 0 and 1).
- `enum` (n variants) → edges `-0.5, 0.5, …, n-0.5` (one cell per variant).
- numeric → B uniform bins over the data range (B = 60 default).

Compute `H = histogram2d(xs, ys, bins=[bx, by])`, then the display field
`Hp = log(1 + H)`. Render `Hpᵀ` as an image with `origin="lower"` (so the y
axis increases upward), nearest-neighbor interpolation, and a sequential
perceptually-uniform colormap (e.g. *inferno*). Colorbar label: `log(1+visits)`.

For discrete axes, place ticks at the integer ordinal positions and label them
with the variant names (or `false`/`true`).

### 4.6 Faceting — small multiples

When a facet var survives §4.2 and the path is discrete:
1. Enumerate the facet's values (`enum_variants` or `[false,true]`).
2. Collect once with facet recorded; partition points by ordinal facet value.
3. Compute each panel's `log(1+H)` and take the **global max** across panels;
   render all panels with a **shared `vmax`** so brightness is comparable
   panel-to-panel (the cardinal rule of small multiples — common scale).
4. One row of panels titled `facet = value`, one shared colorbar.

---

## 5. Variable → channel mapping

| Channel | Variable | Reasoning (Cleveland–McGill / Mackinlay effectiveness) |
|---|---|---|
| **x position** | top-ranked axis (numeric preferred) | position is the highest-accuracy channel; quantitative belongs on position |
| **y position** | 2nd-ranked axis (numeric preferred) | same |
| **color** | the *derived* occupancy density `log(1+visits)` | color encodes the quantity this viz exists to show; it is **not** spent on a state variable |
| **facet** | a low-card, stable categorical config var (≠ axes) | categorical → small multiples is the honest way to add a 3rd dimension to a high-D model without overplotting |
| size, shape | *(unused)* | — |

Key decision distinct from scatter-style views: **color is reserved for the
density, not for a third variable.** A third variable is shown via facets, not
hue, because hue would compete with the density encoding.

---

## 6. Degradation & edge cases

| Situation | Behavior |
|---|---|
| **0 state vars** | placeholder panel "no state variables". |
| **Exactly 1 usable axis** | 1-D occupancy **strip**: histogram of that single axis (40 bins), y = visits; discrete tick labels if enum/bool. Collection still picks the discrete or numeric path. |
| **2 axes, both numeric** | numeric path (grid-seeded trajectories + burn-in). |
| **2 axes, not both numeric, or discrete model** | discrete path (reachable + random walk); non-numeric axes ordinalized per §4.3. |
| **Enum / bool axis** | ordinalized to its variant index / {0,1}; one histogram cell per value; axis ticks labeled with variant names. |
| **String axis** | last-resort hash projection (no metric meaning; cells just separate distinct literals). |
| **No points collected** (transition unsat / no reachable states) | placeholder "no visited states". |
| **Facet present but numeric path** | facet is ignored (faceting only on the discrete reachable path). |

---

## 7. Parameters

| Name | Default | Meaning |
|---|---|---|
| `MAX_FACETS` | 5 | max facet cardinality for small multiples |
| numeric grid resolution G | 9 | per-axis grid points → G² seeds |
| numeric seed span | 3200 | half-width of the seed box per axis |
| trajectory steps S | 120 | steps followed from each numeric seed |
| transient burn-in T | 10 | leading steps discarded before histogramming |
| random-walk steps K | 4000 | dwell-traffic steps on the discrete reachable graph |
| numeric bin count B | 60 | uniform bins per numeric axis |
| 1-D strip bins | 40 | histogram bins for the single-axis fallback |
| intensity transform | `log(1+H)` | log-compress visit counts |
| colormap | sequential perceptual (inferno) | low→dark, high→bright |

---

## 8. References

- **Invariant / occupation measure of a map** — the long-run histogram of an
  orbit approximates the system's invariant measure (Birkhoff ergodic theorem;
  the "natural" / SRB measure for attractors). Standard dynamical-systems
  background: Strogatz, *Nonlinear Dynamics and Chaos*; Ott, *Chaos in
  Dynamical Systems*.
- **Burn-in / transient discarding** — drop initial iterates so the sampled
  distribution reflects the attractor, not the approach; standard in iterating
  maps and in MCMC sampling (Gelman et al., *Bayesian Data Analysis*).
- **2-D histogram density estimation** — Scott, *Multivariate Density
  Estimation* (binned density; a histogram is a piecewise-constant kernel
  density estimate). The log intensity transform is the standard fix for
  heavy-tailed count fields.
- **Random-walk occupation measure on a finite graph** — stationary
  distribution of a Markov chain on the reachable state graph (Levin, Peres &
  Wilmer, *Markov Chains and Mixing Times*).
- **Channel effectiveness** — Cleveland & McGill (1984), "Graphical Perception";
  Mackinlay (1986), "Automating the Design of Graphical Presentations":
  position is most accurate for quantitative data, color/hue for the derived
  density.
- **Small multiples & shared scale** — Tufte, *Envisioning Information* /
  *The Visual Display of Quantitative Information*: faceted panels on a common
  scale as the honest extra dimension.
- **Perceptually-uniform sequential colormaps** — inferno/viridis family
  (Smith & van der Walt), for ordered density without false boundaries.
