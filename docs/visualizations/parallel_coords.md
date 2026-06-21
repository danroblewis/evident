# Parallel Coordinates

> Implements the parallel-coordinates (Inselberg) projection of an Evident
> program's reachable state set. One polyline per sampled state, crossing every
> variable's axis at the height of that state's value on that axis.
>
> Shared transition-query and variable-ranking primitives are defined in
> [`00-core-machinery.md`](00-core-machinery.md) and only *referenced* here.

---

## 1. What it shows

Parallel coordinates render a high-dimensional point set in a 2-D plane without
projection loss. They answer:

- **Which axis-value combinations actually co-occur?** Each visited state is a
  polyline; a renderer reads correlation/anti-correlation between adjacent axes
  from the bundle's slope pattern (parallel segments → positive correlation;
  crossing X-patterns → negative correlation between those two axes).
- **What is the class structure?** When the program has a categorical variable,
  every polyline is colored by its class, so clusters of same-color lines reveal
  which axis-values characterize each class.
- **What region of state space is occupied?** The envelope of all polylines on
  each axis bounds the reachable range of that variable.

**When to use it.** Any program with **≥ 2 state variables** — it is the most
dimension-agnostic of the views and the natural choice when the state has more
than two variables that cannot be reduced to a single 2-D phase plane. Works for
discrete, numeric, and mixed programs; degrades gracefully (see §6). It is *not*
useful for a 1-variable program (no second axis to relate to).

---

## 2. The object

Let the ranked state variables be `V = (v_0, …, v_{d-1})` (d = number of axes)
and let `S` be the sampled set of states. The figure is:

- **d parallel vertical axes**, one per variable, equally spaced at integer
  x-coordinates `x_i = i` for `i = 0 … d-1`. Axis order = importance order
  (`m.state_vars`).
- For each sampled state `s ∈ S`, a **polyline** through the points
  `(i, ŷ_i(s))` for `i = 0 … d-1`, where `ŷ_i(s) ∈ [0,1]` is the normalized
  height of `s`'s value on axis `i`.
- Each axis carries **tick marks** labeling its scale: min / mid / max for a
  numeric axis; every occurring category for a categorical axis.

Mathematically this is the Inselberg point→line duality: a point in `ℝ^d`
becomes a piecewise-linear curve in the (axis-index, normalized-value) plane.

---

## 3. Inputs

From the shared machinery (`00-core-machinery.md`):

- `m.state_vars` — ranked, deduplicated list of state variables (each
  `{name, kind, role}`). Defines axis count, axis order, and per-axis kind.
- `m.is_discrete()` — sampling-strategy switch.
- `m.reachable(limit)` — exact reachable state set (graph BFS over `successor`).
  Used for discrete and finite-cycle mixed programs.
- `m.trajectory(start, steps)` — one successor chain from a seed. Used for
  numeric (continuous) programs where `reachable()` is meaningless on a grid.
- `m.initial_state()` — last-resort seed.
- `m.categorical_vars` — ranked bool/enum/string variables; element 0 drives
  the color channel.
- `m.enum_variants[name]` — declared variant order for an enum (fixes axis tick
  order and legend order).

No `assign_channels` / `facet_var` call — this view's channel assignment is
fixed (see §5).

---

## 4. Algorithm

### 4.1 Collect samples `S`

The dynamics always come from solving the transition; the only choice is *which*
states to draw.

1. **Discrete program** (`is_discrete()` true, i.e. no real/int axes): take the
   exact reachable set `S, _ = reachable()`. Note = "reachable set".
2. **Mixed program** (has a numeric axis *and* a categorical axis): the
   reachable BFS may still be a finite, meaningful cycle (e.g. a vending
   machine). Try `reachable(limit=400)`; if `|S| ≥ 2`, use it. Otherwise fall
   through to step 3.
3. **Numeric / degenerate**: sweep `trajectory(start=seed, steps=120)` from
   several seeds and concatenate the resulting state lists.
   - If axes named `*.x` and `*.v` exist (2-D phase systems, e.g. van der Pol),
     seed from a fixed off-origin grid (`NUMERIC_SEEDS`: a handful of points on
     and off the axes so trajectories sweep a limit cycle rather than sitting at
     a fixed point).
   - Otherwise seed generically: base = all-zero state, then perturb the first
     numeric axis to each of `{-1500, -500, 500, 1500}`.
   - If still empty, take a single long `trajectory(initial_state(), steps=200)`.
   Note = "trajectory sweep".

### 4.2 Build per-axis metadata

For each variable `v_i` with sampled values `vals_i = { s[v_i] : s ∈ S }`:

- **Numeric axis** (`kind ∈ {int, real}`):
  - `lo = min(vals_i)`, `hi = max(vals_i)`. If `lo == hi`, widen to
    `(lo-1, hi+1)` to avoid a zero-width axis.
  - position map `pos(x) = x` (identity — height is the value itself).
  - ticks at `{lo, (lo+hi)/2, hi}`, labeled with the formatted number.
- **Categorical axis** (`kind ∈ {bool, enum, string}`): build an ordered
  category list `cats`:
  - enum → `enum_variants[name]` filtered to those that actually occur (declared
    order preserved); if none occur, keep the full declared list.
  - bool → `[false, true]`.
  - string → `sorted(distinct values)`.
  - Ordinal encoding: `index(c) = position of c in cats`. `lo = 0`,
    `hi = max(1, |cats|-1)`. position map `pos(c) = index(c)`. One tick per
    category at its ordinal, labeled with the category name.

### 4.3 Normalize each axis to [0,1]

For axis `i` and value `val`:

```
p  = pos_i(val)                       # value → position space
ŷ  = (p - lo_i) / (hi_i - lo_i)       # → [0,1]; if hi==lo, ŷ = 0.5
```

This min–max normalization (a standard parallel-coordinates requirement) puts
all axes in one drawing frame regardless of their native units. Each sampled
state becomes a polyline `[(0, ŷ_0), (1, ŷ_1), …, (d-1, ŷ_{d-1})]`.

### 4.4 Color the polylines

Color encodes the **top categorical variable** `c = categorical_vars[0]` if one
exists:

1. Determine the occurring categories of `c` in declared/ordinal order (same
   ordering rule as §4.2).
2. Assign a **qualitative palette**: a 10-color categorical scheme if
   `|cats| ≤ 10`, else a 20-color scheme, indexed mod palette size. Equal hue
   spacing, maximally distinct — this is the class-revealing coloring.
3. Each polyline takes the color of its state's value of `c`.
4. Emit a discrete legend: one swatch per class.

If there is **no categorical variable** (pure-numeric program): color by
**sample order** — a sequential (e.g. viridis) gradient from first to last
sampled state, giving a perceptual sense of time/trajectory progression. Emit a
continuous colorbar instead of a legend.

### 4.5 Draw

For each axis `i`: a vertical line at `x=i`, short tick stubs at each tick
height, right-justified tick labels. Render all polylines as a single line
collection (width ≈ 1.3, alpha ≈ 0.6 so overlapping bundles show density). X-tick
labels = short variable names (drop a leading `state.`/`d.` prefix). Y-axis is
unlabeled (every axis has its own ticks). Plot frame:
`x ∈ [-0.6, d-0.4]`, `y ∈ [-0.05, 1.08]`.

---

## 5. Variable → channel mapping

Parallel coordinates assign **every** state variable to its **own position
axis** — that is the defining property; there is no dimension dropped to color
or facet. The only auxiliary channel is line color:

| Channel | Variable | Reasoning |
|---|---|---|
| **x-axis index** | all of `state_vars`, in importance order | each variable is one parallel axis; order by rank so the most informative relations are adjacent |
| **y-position** (per axis) | that axis's variable | quantitative & ordinal data → position is the most accurate visual channel (Cleveland–McGill) |
| **line color** | `categorical_vars[0]` (else sample-order) | a single categorical class → hue is the effective channel for nominal data; it groups same-class polylines without consuming an axis |

This is deliberately *not* routed through the generic `assign_channels`
helper — the mapping is structural (axes = all vars) and only the color slot is
a choice.

**Adjacency note.** Because correlation between two variables is only readable
when their axes are neighbors, axis *order* matters. This implementation uses
the importance ranking from `state_vars`; a more advanced reimplementation may
reorder axes to minimize total crossings or to place highly-correlated pairs
adjacent (see §8).

---

## 6. Degradation & edge cases

- **< 2 state variables**, or **no samples** from the transition: draw an
  "N/A for this state" placeholder card with the reason; no axes.
- **Discrete**: exact reachable set; categorical axes ordinal-encoded; color by
  top enum/bool. The classic class-colored Inselberg plot.
- **Pure numeric**: trajectory sweep from multiple seeds; no categorical var, so
  color falls back to sample order + colorbar. Axes are true min–max numeric.
- **Mixed**: try the reachable cycle first (finite mixed systems like vending
  machines give a clean small set); fall back to numeric seeding if the BFS is
  degenerate (`|S| < 2`).
- **Constant axis** (`lo == hi`): widened by ±1 (numeric) or normalized to a
  flat 0.5 line so the axis still renders.
- **Enum with unobserved variants**: keep only occurring categories (compacts
  the axis), but preserve declared order for legibility.
- **High `|cats|`**: palette widens from 10→20 colors, then wraps modulo — color
  ceases to be uniquely class-identifying past the palette size, an accepted
  limitation of nominal color encoding.

---

## 7. Parameters

| Parameter | Default | Meaning |
|---|---|---|
| `reachable(limit)` — discrete | 5000 | BFS cap for exact reachable set |
| `reachable(limit)` — mixed | 400 | smaller cap for the mixed-cycle probe |
| mixed-accept threshold | `\|S\| ≥ 2` | min reachable size to prefer BFS over seeding |
| trajectory `steps` (seeded) | 120 | length of each numeric sweep |
| trajectory `steps` (init fallback) | 200 | single long sweep when no seeds |
| numeric phase seeds | `NUMERIC_SEEDS` | 6 off-origin (x,v) points for limit cycles |
| generic numeric perturbations | `{-1500,-500,500,1500}` | first-axis seed offsets |
| qualitative palette switch | `\|cats\| ≤ 10` | 10-color vs 20-color categorical scheme |
| line width / alpha | 1.3 / 0.6 | thin, semi-transparent for density |
| y-frame | `[-0.05, 1.08]` | normalized value range + label headroom |

---

## 8. References

- **A. Inselberg**, *Parallel Coordinates: Visual Multidimensional Geometry and
  Its Applications* (Springer, 2009); Inselberg & Dimsdale, "Parallel
  Coordinates: A Tool for Visualizing Multi-Dimensional Geometry" (IEEE Vis,
  1990) — the point↔polyline duality and the correlation-by-slope reading.
- **E. Wegman**, "Hyperdimensional Data Analysis Using Parallel Coordinates"
  (JASA, 1990) — statistical interpretation, density via line overplotting.
- **W. Cleveland & R. McGill**, "Graphical Perception" (JASA, 1984) — position is
  the most accurate channel for quantitative data; justifies all-vars-to-axes.
- **J. Mackinlay**, "Automating the Design of Graphical Presentations" (ACM
  TOG, 1986) — channel-effectiveness ranking (nominal→color, quantitative→
  position) behind §5.
- **Axis-ordering** (for a smarter reimplementation): Ankerst, Berchtold &
  Keim, "Similarity Clustering of Dimensions for an Enhanced Visualization of
  Multidimensional Data" (IEEE InfoVis, 1998) — reorder axes to place correlated
  variables adjacent and reduce crossings.
- Dynamics sampling (reachable BFS, trajectory chains, successor solves):
  see [`00-core-machinery.md`](00-core-machinery.md).
