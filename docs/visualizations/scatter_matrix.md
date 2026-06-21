# Scatter Matrix (Pairwise Projection Grid)

A scatterplot matrix — also called a SPLOM, pairs plot, or draftsman's display —
of an Evident program's state space. Given the program's *N* ranked state
variables, it draws an *N×N* grid of 2-D scatter panels: panel *(i, j)* projects
a cloud of sampled states onto the plane (variable *j* on x, variable *i* on y).
The diagonal carries the variable name and a 1-D marginal histogram. A single
categorical variable hues every point as a third (color) channel.

This document specifies only the scatter-matrix algorithm. The shared transition
queries, variable ranking, and channel machinery it consumes are defined in
[`00-core-machinery.md`](00-core-machinery.md) and referenced by name below.

---

## 1. What it shows

The scatter matrix answers: **what is the joint structure of the reachable state
cloud, viewed through every pair of variables at once?** Each off-diagonal panel
exposes pairwise correlation, clustering, and the geometry of the attractor /
reachable set in that 2-D projection; the diagonal shows each variable's marginal
distribution. Reading down a column or across a row lets you compare how one
variable relates to all the others.

It is the **general-purpose, dimension-agnostic** view: unlike a phase portrait
(which needs exactly two strong numeric axes) it scales to arbitrary *N* and to
any mix of kinds, because every variable is ordinalized onto a real axis.

- **Numeric programs** — panels show the continuous attractor / vector-field
  image as point clouds; correlations and limit cycles appear as structure.
- **Discrete programs** — panels show the reachable state graph's vertices
  projected pairwise; clusters = strongly-connected regions.
- **Mixed programs** — bool/enum axes become small ordinal ladders; the
  categorical color channel separates regimes within the numeric panels.

Use it as the *survey* visualization when you do not yet know which two variables
matter, or when *N > 2* and no single 2-D plot suffices.

---

## 2. The object

Let the ranked state variables be `v_0, …, v_{N-1}` (from `state_vars`, see
core-machinery §ranking). Let `S = {s_1, …, s_M}` be a finite **cloud of sampled
states** (§4.1). Define an ordinal embedding

```
φ_v : value(v) → ℝ
```

(§4.2) that maps any state value onto a real coordinate. The rendered object is
the *N×N* matrix of panels `P[i][j]`:

- **Off-diagonal (i ≠ j):** the 2-D point set
  `{ ( φ_{v_j}(s[v_j]), φ_{v_i}(s[v_i]) )  :  s ∈ S }`,
  i.e. the orthogonal projection of the embedded cloud `Φ(S) ⊂ ℝ^N` onto the
  coordinate plane spanned by axes *j* (x) and *i* (y).
- **Diagonal (i = j):** the 1-D marginal `{ φ_{v_i}(s[v_i]) : s ∈ S }`, rendered
  as a histogram with the variable name overlaid.

Each point carries a **color** `χ(s) ∈ palette` determined by one categorical
variable (§5). The matrix is symmetric in *content* (panel *(i,j)* is the mirror
of *(j,i)*) but both halves are drawn; this is the standard redundant-but-readable
SPLOM convention.

---

## 3. Inputs (core-machinery primitives consumed)

| Primitive | Use |
|---|---|
| `state_vars` | The ordered, deduplicated list of variables → matrix rows/cols, ranked by importance. |
| `is_discrete()` | Branch the sampler: graph BFS vs. trajectory+grid sweep. |
| `reachable(limit)` | Discrete path: the vertex set of the reachable transition graph. |
| `initial_state()` | Seed for trajectories and base for grid pinning. |
| `trajectory(start, steps)` | Numeric path: follow successor chains to expose the attractor. |
| `successor(state)` | Numeric path: one map step from each grid lattice point (vector-field image). |
| `categorical_vars` | The color channel: `categorical_vars[0]` is the hue variable. |
| `enum_variants[name]` | Stable variant ordering for enum ordinalization and color categories. |

No SMT encoding is touched directly; every dynamical fact comes from these
queries.

---

## 4. Algorithm

### 4.1 Sample the state cloud `S`

The goal is a representative point cloud, sampled differently per program shape.

1. **Discrete** (`is_discrete()` true): run `reachable(limit = 5000)` → `(states,
   edges)`. If non-empty, `S = states`. (Edges are available but not drawn by the
   base SPLOM; a renderer may overlay them.)
2. **Numeric / mixed:** build `S` as the union of two sources, to capture *both*
   the long-run attractor and the transient vector field:
   - **(a) Trajectory bundle.** Collect seeds: the `initial_state()`, plus — if
     ≥ 2 numeric variables exist (kind ∈ {int, real}) — a small fixed set of
     spread-out seeds formed by overwriting the two top numeric vars `(a, b)`
     with lattice corners (default set: `(2800,0), (400,0), (0,2700),
     (-1500,1500), (1200,-1200)`), leaving all other fields at the init value.
     From each seed take `trajectory(start = seed, steps = 400)` and append every
     visited state. Multiple seeds reveal distinct basins / limit cycles.
   - **(b) Grid sweep (vector-field image).** If ≥ 2 numeric vars: over a square
     lattice of the top-two numeric axes `(a, b)` (default range `[-3200, 3200]`,
     step `800` → 9×9 = 81 points), pin `a, b` to each lattice point (other
     fields at init/base), compute one `successor`, and append **both** the
     pinned pre-state and its successor. This draws the map's action on a uniform
     grid, so panels show *f(x)* structure, not only the attractor the
     trajectories settle onto.

If `S` is empty after sampling, degrade (§6).

### 4.2 Ordinal embedding `φ_v`

Map each state value to a real axis coordinate by kind:

| kind | `φ_v(value)` |
|---|---|
| `int`, `real` | `float(value)` |
| `bool` | `1.0` if true else `0.0` |
| `enum` | index of `value` in `enum_variants[v]` (variant declaration order) |
| `string` | `0.0` (placeholder — strings have no metric axis; see §6) |

Precompute one column per variable: `cols[v] = [ φ_v(s[v]) for s ∈ S ]`. This is
O(N·M) and is the only embedding pass; all panels read from these columns.

**Categorical tick decoration.** For bool and enum axes, attach human-readable
tick marks at the ordinal positions: bool → `{0:"F", 1:"T"}`; enum → variant
names at `0..k−1`. Apply only when the ladder is short (≤ 8 ticks) to stay
legible; otherwise leave the axis numeric. Tick *labels* are shown only on the
matrix's outer edge (bottom row for x, left column for y) to avoid clutter.

### 4.3 Build the matrix

For each `(i, j)` in `N × N`:

- **Diagonal (i = j):** draw a marginal histogram of `cols[v_i]` (default 15
  bins) as a light backdrop; overlay the variable name centered. No ticks.
- **Off-diagonal:** scatter `x = cols[v_j]`, `y = cols[v_i]` with per-point color
  `χ` (§5), small markers, partial transparency (default alpha ≈ 0.45) so dense
  regions read as saturation — a poor-man's density estimate.

Panel side length scales as `max(2.0, 12.0 / N)` inches so the figure stays
bounded as *N* grows; font sizes shrink with *N* (`max(8, 14−N)` for diagonal
labels, etc.).

### 4.4 Complexity

Sampling dominates (each `successor`/step is one SMT solve). Embedding and
drawing are O(N²·M). For large *N* the grid of N² panels, not the math, is the
practical limit.

---

## 5. Variable → channel mapping

This visualization deliberately puts **every** variable on the strongest channel,
position, by carrying it pairwise:

| Channel | Assignment | Type-effectiveness reasoning |
|---|---|---|
| **x / y (position)** | every variable, once per row and once per column | Position is the most accurately decoded channel (Cleveland–McGill); the SPLOM's whole premise is that *all* pairwise positions are worth showing, so no per-variable fitness gate is applied. Categorical vars sit on short ordinal ladders. |
| **color (hue)** | `categorical_vars[0]` — the top enum/bool/string var | A categorical attribute maps naturally to a nominal palette; hue carries one extra dimension across *all* panels at once. This is the classic high-D SPLOM coloring. |
| **size** | unused (fixed marker size) | Reserved; SPLOM density is conveyed by alpha-blending, not size. |
| **facet** | unused | The matrix *is* the faceting (small multiples over variable pairs); a second facet would nest grids. |

Note this viz does **not** call `assign_channels` — it bypasses the fitness-gated
mapping because its mandate is exhaustive pairwise position. It uses only
`categorical_vars[0]` for the optional color enhancement.

**Color category construction.** Given `cat = categorical_vars[0]`:
- enum → categories = `enum_variants[cat]` (declaration order);
- bool → `[False, True]` labeled `F`/`T`;
- string → the *distinct values that actually occur in S*, sorted as strings.

Assign each category a palette index (default `tab10` for ≤ 10 categories, else
`tab20`), so `χ(s) = palette[ index_of(s[cat]) ]`; unseen/unknown values get a
neutral gray. Build **one figure-level legend** listing only categories that
occur in `S`, placed outside the grid. The panels remain readable from their axes
alone — color only *enhances*. If the model has no categorical variable
(`categorical_vars` empty), use a single flat color and emit no legend.

---

## 6. Degradation & edge cases

- **Empty cloud (`S = ∅`) or `N = 0`:** render a placeholder panel reading
  `N/A … no sampled states`. The system is unsatisfiable / has no reachable
  states to project.
- **Single variable (`N = 1`):** no pairwise plane exists. Fall back to a single
  1-D histogram of `cols[v_0]` (default 20 bins) with categorical ticks if
  applicable, annotated "scatter matrix needs ≥ 2 vars".
- **No categorical variable:** color channel is dropped (flat hue, no legend);
  the matrix is purely positional.
- **String variables:** ordinalize to the constant `0.0` (no metric meaning), so
  their off-diagonal panels collapse to a line — a deliberate, honest placeholder
  rather than an invented embedding. A string may still drive the **color**
  channel meaningfully via its occurring-values set.
- **Numeric program with < 2 numeric vars:** the seed-spreading and grid-sweep
  steps are skipped (they require two numeric axes to pin); `S` reduces to the
  init-seeded trajectory alone. The matrix still renders over whatever vars exist.
- **Large *N*:** panels shrink and tick labels are suppressed on interior cells;
  categorical tick labels appear only on the outer frame.

---

## 7. Parameters

| Parameter | Default | Meaning |
|---|---|---|
| `reachable` limit | 5000 | Max vertices for the discrete cloud (BFS cap). |
| trajectory steps | 400 | Successor-chain length per seed (numeric path). |
| seed corners | `(2800,0),(400,0),(0,2700),(-1500,1500),(1200,-1200)` | Spread seeds over the top-2 numeric axes. |
| grid range / step | `[-3200, 3200]`, step `800` (→ 9×9) | Lattice for the vector-field successor sweep. |
| diagonal histogram bins | 15 | Marginal density resolution on the diagonal. |
| 1-D fallback bins | 20 | Histogram bins when `N = 1`. |
| scatter alpha | ≈ 0.45 | Transparency → density-by-saturation. |
| marker size | ~10 px | Off-diagonal point size (fixed). |
| panel side | `max(2.0, 12/N)` in | Per-panel figure size. |
| categorical tick cap | ≤ 8 | Max ladder length to draw category ticks. |
| color palette | `tab10` (≤10) / `tab20` | Nominal hue scale for the color category. |

The seed corners and grid bounds assume integer state spaces on the order of
thousands; a reimplementation should scale these to the model's actual numeric
range (e.g. derive from observed min/max in the trajectory bundle).

---

## 8. References

- **Scatterplot matrix / draftsman's display.** Cleveland, W. S. (1985/1993),
  *The Elements of Graphing Data* and *Visualizing Data* — the SPLOM as the
  canonical multivariate survey plot; brushing and conditioning.
- **Graphical perception / channel ranking.** Cleveland & McGill (1984),
  "Graphical Perception: Theory, Experimentation, and Application to the
  Development of Graphical Methods," *JASA* — position as the most accurately
  decoded channel; hue as a nominal channel. Underpins the position-for-all,
  hue-for-one mapping.
- **Small multiples.** Tufte, E. (1983), *The Visual Display of Quantitative
  Information* — the matrix of panels as small multiples over variable pairs.
- **Alpha-blending as density.** Wegman, E. (1990), "Hyperdimensional Data
  Analysis Using Parallel Coordinates," and standard practice in
  overplotting-mitigation — transparency as a kernel-free density proxy.
- **Ordinal encoding of categoricals.** General data-visualization practice
  (e.g. Wilkinson, *The Grammar of Graphics*, 2005) — mapping nominal/ordinal
  values to integer positions for display.
- **Reachable-set sampling for dynamical systems.** Strogatz, *Nonlinear
  Dynamics and Chaos* (1994) — trajectories reveal attractors/limit cycles; the
  grid sweep is a discrete sampling of the map's image f(x), complementing the
  attractor view.
