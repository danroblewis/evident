# nullcline_field — qualitative-flow / sign-region phase plane

A language-agnostic specification. Reimplementable from this doc plus
`docs/visualizations/00-core-machinery.md` (the shared transition-query and
variable-ranking primitives — referenced here by name, not re-documented), given
an SMT solver and a 2-D drawing surface.

---

## 1. What it shows

The **qualitative phase-plane analysis** of an Evident program's transition
relation, read directly off the map (no symbolic differentiation, no
hardcoding). For each point of a 2-D grid in two state coordinates it asks the
solver "what is the successor?", forms the **displacement** `Δ = f(s) − s`, and
draws three superimposed layers:

- **Sign regions** — the plane is shaded by the *sign pattern* of the two
  displacement components (which way each coordinate is moving).
- **Nullclines** — the zero-level curves of each component (`Δx = 0`, `Δy = 0`),
  i.e. the loci where one coordinate momentarily stops changing.
- **Flow + fixed points** — a unit-normalized quiver of the displacement field,
  and the nullcline intersections (where both components vanish) marked as
  approximate fixed points.

Use it for programs with a **continuous coordinate**: it answers "where does the
state want to flow, where does it rest, and what is the global structure of the
dynamics (sinks, sources, limit cycles)?" It is the natural view for oscillators
and continuous controllers (e.g. a van-der-Pol-shaped position/velocity system).
For purely discrete programs it has no defined object (see §6).

---

## 2. The object

Let the transition relation be a (possibly nondeterministic, but here treated as
a function via `successor`) map `f` on the full carried state. Project onto two
chosen coordinates `x` and `y`. Define the **discrete displacement field**

```
Δ(x, y) = ( Δx, Δy ) = ( f(x,y)_x − x ,  f(x,y)_y − y )
```

evaluated over a regular grid. The rendered object is:

1. **Sign-region field** `R(x,y) = 𝟙[Δx ≥ 0] + 2·𝟙[Δy ≥ 0] ∈ {0,1,2,3}`, a
   4-color partition of the plane (the four quadrant-signatures of the flow).
2. **Nullclines** `{Δx = 0}` and `{Δy = 0}` — the boundaries between sign
   regions, drawn as the zero-contours of `Δx` and `Δy` respectively.
3. **Vector field** `Δ/‖Δ‖` (direction only) at a subsampled grid.
4. **Fixed points** `{(x,y) : ‖Δ‖ ≈ 0}` — nullcline intersections.

This is exactly the textbook **phase-plane / nullcline analysis** of a planar
dynamical system, with the continuous derivative `ẋ = g(x)` replaced by the
one-step displacement `Δ = f(s) − s` of the difference equation.

---

## 3. Inputs (core-machinery primitives consumed)

From `00-core-machinery.md`:

- `numeric_vars` — ranked quantitative state variables; supplies the position
  axes (this viz **bypasses** the generic `assign_channels` mapping; see §5).
- `categorical_vars`, `enum_variants` — for the mixed/faceted case (ordinal Y
  axis and facet partition).
- `facet_var(max_card, max_change)` — selects a **near-static** categorical to
  facet on (a configuration set once, *not* a coordinate that varies along the
  trajectory).
- `successor(state)` — the single load-bearing query: pin a previous state,
  solve for the next. Called once per grid cell. (For partial states in the
  mixed case, fill carried-but-unpinned variables with benign type defaults so
  the previous state is total before querying.)
- `reachable(limit)` — used only in the mixed case to derive an honest integer
  range for the lone numeric axis from actually-reachable states.
- `initial_state` is **deliberately not** used to seed the window — the initial
  state of a limit-cycle system is often a fixed point and would collapse the
  extent.

---

## 4. Algorithm

### 4A. Dispatch by numeric dimensionality

1. Let `nums = numeric_vars`, `cats = categorical_vars`.
2. If `|nums| ≥ 2`: **two-axis sign-field** (§4B) on `nums[0]` (X), `nums[1]` (Y).
3. Else if `|nums| = 1` and `facet_var(max_card=6, max_change=0.25)` returns a
   variable: **faceted mixed sign-field** (§4C).
4. Else: **placeholder** (§6) — the sign of `Δ` is undefined without a continuous
   coordinate.

### 4B. Two-axis sign-field (canonical case)

1. **Derive the plotting window** by probing, not from the initial state. For
   each pair `(px, pv)` drawn from a fixed symmetric probe set `PROBES` (default
   `{±3200, ±2800, ±1600, ±800, ±400, 0}`), query `successor({x:px, y:pv})`.
   Accumulate both each accepted seed and its successor's `(x,y)`. Take
   `[min,max]` over the accumulated values per axis. (Including successors in the
   window guarantees the visible flow lands inside the frame.) If nothing was
   accepted, fall back to a symmetric default window.
2. **Pad** each axis: center `c = ½(lo+hi)`, half-range `r = max(1, ½(hi−lo))·PAD`
   (`PAD = 1.10`), window `[c−r, c+r]`.
3. **Sample the grid.** `xs = linspace(xlo, xhi, GRID)`, `ys = linspace(ylo, yhi,
   GRID)` (`GRID = 41`). For each cell `(i,j)`, round to integer coordinates, query
   `successor`, and store `Δx[j,i] = x' − x`, `Δy[j,i] = y' − y`. Cells where the
   solver returns UNSAT are left as NaN and skipped everywhere downstream.
4. **Sign-region field.** `region = 𝟙[Δx ≥ 0] + 2·𝟙[Δy ≥ 0]` on non-NaN cells;
   render as a 4-color raster (nearest-neighbor, no interpolation) over the window
   extent. Color legend: `(↑,↑)`, `(↓,↑)`, `(↑,↓)`, `(↓,↓)` for the four codes.
5. **Nullclines.** Draw the zero-level contour of `Δx` (one color) and of `Δy`
   (another), each as a marching-squares isocurve at level 0 over the
   `meshgrid(xs, ys)`. These are the region boundaries; their **crossings** are
   the fixed points.
6. **Flow quiver.** Subsample by `step = max(1, GRID // 18)`. At each subsampled
   cell compute magnitude `mag = hypot(Δx, Δy)` and plot the **unit** vector
   `Δ/mag` (zero where `mag = 0`). Direction only — magnitude is intentionally
   discarded so slow and fast regions read identically (this is a *qualitative*
   field).
7. **Fixed points.** Mark cells with `|Δx| ≤ tol(Δx)` and `|Δy| ≤ tol(Δy)`, where
   the per-axis tolerance is `tol(a) = max(1, 0.12 · P60(|a|))` and `P60` is the
   60th percentile of the finite absolute displacements on that axis. This is a
   scale-adaptive "≈ 0" band that survives integer-rounded grids.

### 4C. Faceted mixed sign-field (one numeric + categoricals)

The honest dimension-add for a single continuous axis is **faceting**, not a
synthetic second numeric axis.

1. **Facet levels** = the ordered values of `facet` (`bool → [false, true]`;
   `enum → enum_variants[facet]`). One panel per level.
2. **Y axis** = a *second* categorical `yv` distinct from the facet (first one in
   `cats` whose name ≠ facet's), ordinalized: `bool → {0,1}`, `enum → variant
   index`. If none exists, Y degenerates to a single row.
3. **X axis** = the lone numeric var over its **reachable integer range**:
   compute `reachable`, take `[min,max]` of that var across reachable states, cap
   the span at 32 cells (`hi = lo + 32` if wider), enumerate the integers. Falls
   back to `0..3` if reachability yields nothing.
4. **Per panel**, for each `(x-cell, y-level)`: build the partial previous state
   `{xv:x, facet:fval, yv:ylevel}`, fill remaining carried vars with type
   defaults, query `successor`, and store `Δx = x' − x`.
5. **Sign coloring.** Only the *one component with a continuous axis* is signed:
   3-color raster on `sign(Δx) ∈ {−1, 0, +1}` (↓ / nullcline / ↑).
6. **Nullcline + flow markers.** Cells with `Δx = 0` are the nullcline, marked
   with a dot; otherwise draw a small horizontal arrow pointing in the direction
   of `sign(Δx)` (a per-cell 1-D flow glyph along the numeric axis).

---

## 5. Variable → channel mapping

This viz **overrides** the generic channel assignment because its object is
*structural*: it needs two POSITION axes for a 2-D field and a *derived*
quantitative color.

| Channel | Two-axis case | Faceted mixed case |
|---|---|---|
| **X** (position) | `numeric_vars[0]` | the one numeric var |
| **Y** (position) | `numeric_vars[1]` | second categorical, ordinalized |
| **Color** | `sign(Δx, Δy)` (derived, 4 regions) | `sign(Δx)` (derived, 3 levels) |
| **Facet** | — | near-static categorical (`facet_var`) |

Type-effectiveness reasoning (Cleveland–McGill / Mackinlay): quantitative
variables go on **position** because position is the highest-accuracy channel for
magnitude, and the sign-field is fundamentally a function of two continuous
coordinates. The *color* is not a raw variable — it encodes the **derived sign of
change**, a genuinely informative ordinal/nominal quantity, so it is kept rather
than clobbered with a variable hue. Categoricals, which have low effectiveness on
position, drive **facet** (a near-static regime) and an **ordinal Y** (low
cardinality, encoded as variant index) — both honest uses of a nominal channel.

The facet must be **near-constant along a run** (`max_change = 0.25`): faceting on
a variable that changes on the trajectory (e.g. a limit-cycle mode bit) would
split a single cycle across panels and destroy the very structure the viz exists
to show.

---

## 6. Degradation & edge cases

- **≥ 2 numeric:** full two-axis field (§4B). The richest, canonical output.
- **1 numeric + suitable facet:** faceted mixed field (§4C). A single continuous
  coordinate still reads as a per-mode sign line.
- **1 numeric, no suitable facet** *(or)* **0 numeric (purely discrete):**
  **placeholder** panel stating "nullcline_field needs a numeric axis (sign of
  Δvar requires a continuous coordinate)", listing the state-var kinds. The
  sign-of-change object is genuinely undefined without a continuous axis — no
  silent fake projection.
- **UNSAT cells** (no successor for a pinned previous state, e.g. out-of-domain
  probes): left NaN and skipped in shading, contours, quiver, and fixed-point
  detection.
- **Empty window** (no probe accepted): symmetric default extent.
- **Integer rounding:** grid coordinates are rounded to integers before pinning
  (state variables may be integer-typed); the percentile-scaled fixed-point
  tolerance (§4B.7) absorbs the resulting granularity.

---

## 7. Parameters

| Name | Default | Meaning |
|---|---|---|
| `GRID` | 41 | grid samples per numeric axis (two-axis case) |
| `PAD` | 1.10 | window padding factor beyond seed-derived range |
| `PROBES` | `{±3200, ±2800, ±1600, ±800, ±400, 0}` | seed spread for window derivation |
| quiver `step` | `max(1, GRID//18)` (≈ 2) | subsample stride for the flow field |
| fixed-point tol | `max(1, 0.12·P60(|Δ|))` per axis | scale-adaptive "≈ 0" band |
| facet `max_card` | 6 | max categorical cardinality to facet on |
| facet `max_change` | 0.25 | max along-trajectory change rate to count as "static" |
| reachable `limit` | 2000 | cap on reachable states for the mixed X range |
| X-range cap | 32 | max integer cells on the mixed numeric axis |

---

## 8. References

- **Phase-plane / nullcline analysis.** Strogatz, *Nonlinear Dynamics and
  Chaos* (2014), ch. 5–7 — nullclines as zero-curves of each velocity component,
  their intersections as fixed points, and the sign-region partition of the
  plane. The van der Pol oscillator (the limit-cycle exemplar) is the motivating
  shape.
- **Difference-equation phase portraits.** The continuous vector field `ẋ` is
  replaced by the one-step displacement `Δ = f(s) − s`; sign-region and nullcline
  structure carry over to maps (cf. discrete dynamical-systems treatments, e.g.
  Galor, *Discrete Dynamical Systems*).
- **Direction (unit) fields.** Normalizing to `Δ/‖Δ‖` for a purely qualitative
  flow is the standard slope/direction-field convention (Borrelli & Coleman,
  *Differential Equations: A Modeling Perspective*).
- **Channel effectiveness.** Cleveland & McGill (1984), "Graphical Perception";
  Mackinlay (1986), "Automating the Design of Graphical Presentations" —
  quantitative → position, nominal → color/facet ranking that justifies §5.
- **Small multiples / faceting.** Tufte, *Envisioning Information* (1990) — the
  faceted mixed case as a panel-per-regime small-multiple.
