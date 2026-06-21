# Phase Portrait

A phase portrait of an Evident program viewed as a **difference equation** (a
discrete dynamical system). Each state is a point in a 2-D plane; the runtime's
transition relation is the *map* `f` that sends a state to its successor. The
picture shows the **displacement field** `f(x) - x` over the plane, plus real
trajectories, fixed points, and absorbing states.

All transition queries, variable ranking/dedup, channel effectiveness, and the
facet guard are defined in [`00-core-machinery.md`](00-core-machinery.md). This
doc specifies only the phase-portrait-specific geometry and algorithm.

---

## 1. What it shows

**The question:** *Where does the dynamics flow?* Given the program's
state-to-successor map, in which direction and how fast does each region of
state-space move under one tick, where are the fixed points / attractors, and
what do orbits look like?

**When to use it.** It is the default "dynamical systems" view and adapts to the
program shape:

- **Numeric** (≥ 2 int/real state vars): a *continuous vector field* over
  value-space. This is the canonical phase portrait — best for oscillators,
  integrators, growth/decay maps, anything with a real-valued continuum.
- **Discrete** (0–1 numeric vars; bools/enums dominate): a *projected
  transition graph* — real arrows between reachable states.
- **Mixed** (exactly 1 numeric var + categoricals): the numeric var is one
  axis, the highest-cardinality categorical is the other; drawn as a transition
  graph because there is no 2-D continuum to sample.

---

## 2. The object

Let the chosen axes be variables `X` and `Y`, each projected to a real number by
an encoding `π` (Section 4.2). The plane is `(π(X), π(Y))`.

- **Numeric regime.** A *normalized vector field*: at a grid point `p = (x, y)`,
  compute `s = successor(state with X=x, Y=y, others pinned)`, and draw an arrow
  with direction `(π(X(s)) - x, π(Y(s)) - y)` (the displacement `f(p) - p`),
  unit-normalized for legibility, colored by the raw displacement magnitude
  `‖f(p) - p‖`. Overlaid are several **orbits** (trajectories) and **fixed
  points** (grid cells where `f(p) = p`).

- **Discrete / mixed regime.** The *image of the difference equation on its
  reachable set*: nodes are reachable states projected to `(π(X), π(Y))`, directed
  edges are the real transitions `x → f(x)`. Self-loop-only nodes are
  **absorbing states**. The global initial state is ringed.

A **facet** dimension (one low-cardinality categorical) may lift a third
variable off the plane into a panel grid — one panel per categorical value —
rather than overloading color or jitter (position is the strong channel; faceting
is the honest dimension-adder for categoricals; see Cleveland & McGill, Mackinlay).

---

## 3. Inputs (core-machinery primitives consumed)

From the shared `Model` (`00-core-machinery.md`):

- `state_vars` — ranked, deduplicated state variables (the axis pool).
- `numeric_vars` — the int/real subset, in rank order (drives axis selection).
- `enum_variants[name]` — ordered variant list (ordinal encoding + tick labels).
- `facet_var(max_card, max_change)` — the suitable facet candidate
  (low-cardinality categorical, low within-run change rate), or `None`.
- `initial_state()` — first-tick state; seeds the pin and the numeric sampling
  range; marks the initial node.
- `successor(state)` — one step of the map `f`; the field/arrow primitive.
- `trajectory(start, steps)` — one successor chain; the overlaid orbits.
- `reachable(limit)` — BFS reachable `(states, edges)`; the discrete graph.
- `_key(state)` — canonical state key (identity / initial-node matching).

Per-variable `kind` (`int`/`real`/`bool`/`enum`/`string`) drives the projection.

---

## 4. Algorithm

### 4.1 Channel planning (`plan_channels`)

Decide axes, optional facet, and regime:

1. If `len(numeric_vars) ≥ 2`: axes `= numeric_vars[0], numeric_vars[1]`.
   Regime `= numeric`. Facet `= facet_var()` *unless* it is already an axis
   (then `None`). (We deliberately bypass generic channel assignment here — a
   true field needs *two numeric axes*, not a categorical on `y`.)
2. Else (< 2 numerics → discrete/mixed):
   a. Take `facet = facet_var()` first; remove it from the axis pool.
   b. Axis pool `= state_vars` minus the facet name.
   c. If the pool has < 2 vars, drop the facet and reuse all `state_vars`.
   d. If still < 2 vars → regime `degenerate` (placeholder figure, stop).
   e. Order the pool: **numeric vars first**, then categoricals sorted by
      **descending cardinality** (an enum spreads the axis more than a bool).
      Axes `= first two`. Regime `= mixed` if any numeric in the pool, else
      `discrete`.

### 4.2 Value → plane projection `π` (`_numeric`)

- `int` / `real` → the value as a float.
- `bool` → `1.0` if true else `0.0`.
- `enum` → `enum_variants[name].index(value)` (ordinal position).
- `string` (and unknown) → `0.0` (collapses; strings are poor axes).

Cardinality for axis ordering: enum → variant count; bool → 2; numeric → a large
sentinel (treated as high-resolution). Categorical axes get tick
positions/labels (`_axis_ticks`): bool → `{0,1}`↦`{false,true}`; enum →
`range(k)` ↦ variant names.

### 4.3 Numeric field panel (`render_numeric_panel`)

Inputs: axes `X, Y`; a `pin` dict fixing **every non-axis var** (facet value +
off-axis carried vars) so we sweep a clean 2-D slice.

1. **Sampling range.** For each axis, probe a range (`_value_range`): collect the
   initial-state value plus `successor` outputs at seed magnitudes
   `{100, 1000, 3000}` (other vars pinned); take `mag = max |value|`; range is
   `±1.4·mag` (floor `mag = 10` if tiny).
2. **Square the window.** `span = max(|ranges|, 3200)` — a floor so wide-orbit
   oscillators (e.g. Van der Pol) are not clipped. Set both axes to `[-span,
   span]`.
3. **Grid.** `n = 21` points per axis → a `21×21` uniform grid (`linspace`).
4. **Field at each grid point.** Build `state = pin ∪ {X=x, Y=y}` (round to int
   for int-kind axes), query `successor`. Skip points where it is `None`
   (infeasible). Displacement `(dx, dy) = (π(X(s))-x, π(Y(s))-y)`; magnitude
   `‖(dx,dy)‖`.
5. **Fixed points.** A grid cell with `|dx|, |dy| < 1e-9` **and** strictly
   interior (`|x|, |y| < 0.92·span`, to avoid boundary artifacts) is a fixed
   point of `f`. Star them in red.
6. **Draw arrows.** Plot each arrow at `(x, y)` with direction `(dx, dy)/‖·‖`
   (unit-normalized so direction stays readable regardless of speed), colored by
   the *raw* magnitude on a sequential colormap (perceptually-ordered, e.g.
   viridis), pivoted at the cell center.
7. **Overlaid orbits.** From 6 off-origin seeds — `(0.85·xhi,0)`, `(0.12·xhi,0)`,
   `(0,0.85·yhi)`, `(-0.45·xhi,0.55·yhi)`, `(-0.85·xhi,0)`, `(0,-0.85·yhi)` —
   call `trajectory(start, steps=400)`, project each visited state, and draw a
   polyline (skip if < 2 points). The off-origin placement matters: the origin
   is often the fixed point of an oscillator and a centered seed never moves.
   Mark each seed start with a ringed dot.

### 4.4 Discrete / mixed panel (`render_discrete_panel`)

Inputs: axes `X, Y`; a list of state dicts `states`; edges as index pairs `(i,j)`
into that list; the global initial-state key; shared axis bounds.

1. **Project + collision-spread.** For each state compute `(π(X), π(Y))`. Many
   distinct states can collapse to the same 2-D point (the axes are a *projection*
   of a higher-dim state). Track an occupancy count `k` per coordinate and place
   the `k`-th occupant on a **phyllotaxis (vortex) spiral**: angle `k·2.399963`
   rad (the golden angle ≈ 137.5°), radius `0.10 + 0.06·k`. This deterministically
   declutters overlapping states without overlap bias.
2. **Successor sets + absorbing.** Build `succ[a] = {b : (a,b) ∈ edges}`. A node
   `a` is **absorbing** iff `succ[a] = {a}` (its only successor is itself).
3. **Edges.** For every non-self edge, draw a directed arrow from `place(a)` to
   `place(b)` (slightly shrunk at both ends so arrowheads clear the node markers).
4. **Nodes.** Scatter non-absorbing states as filled dots; absorbing states as
   large red stars.
5. **Initial node.** The state whose `_key` equals the global initial key gets a
   bright open ring.

### 4.5 Faceting (panel grid)

When a facet var exists, panel values are the categorical domain (`_facet_values`:
enum → all variants; bool → `[false, true]`). Layout: `cols = min(n, 3)`,
`rows = ⌈n/cols⌉`; unused cells blanked.

- **Numeric facet.** For each value, copy the pin, set `facet = value`, render a
  field panel (Section 4.3). Share one magnitude colorbar across panels; share
  axis ranges via the square window.
- **Discrete facet.** A state belongs to the panel of its facet value. An edge is
  drawn **only if both endpoints share the facet value**; cross-facet edges are
  *counted and annotated*, never drawn — an in-plane arrow between panels would be
  geometrically dishonest (it would need a 3rd axis). All panels share global axis
  bounds (`_bounds_of`, padded) so positions are comparable.

### 4.6 Orchestration (`render`)

`degenerate` → titled placeholder ("needs 2 axes"). `numeric` → `_render_numeric`
(single panel or faceted grid). `discrete`/`mixed` → `_render_discrete`, which
first calls `reachable(limit=3000)`; if empty, emit an "no reachable states"
placeholder.

---

## 5. Variable → channel mapping

| Channel | Numeric regime | Discrete / mixed regime | Reasoning |
|---|---|---|---|
| **x** | `numeric_vars[0]` | first of (numerics, then high-card enums) | quantitative → position (the strongest channel) |
| **y** | `numeric_vars[1]` | second of that ordering | second strongest position channel |
| **color** | step magnitude `‖f(p)-p‖` (sequential) | absorbing/initial/normal class | the one good quantitative use of hue is an ordered scalar; categorical state-role is a small nominal palette |
| **size** | (uniform) | absorbing stars enlarged | size reserved for emphasis, not data |
| **facet** | a suitable low-card categorical (≤ ~5 values), not on an axis | same | categoricals add a dimension best by small multiples, not by overloading one plane |

Type-effectiveness rationale (Cleveland–McGill / Mackinlay): quantitative
variables go to **position**; categoricals that stay ~constant within a run go to
**facet**; only a derived *ordered scalar* (the field magnitude) is allowed on
hue. A categorical on the *limit cycle* (high within-run change rate) is rejected
as a facet by `facet_var` — faceting it would slice the dynamics across panels
instead of keeping each orbit inside one.

---

## 6. Degradation & edge cases

- **< 2 distinguishable axes** → `degenerate`: a titled placeholder, no plot.
- **No reachable states** (initial_state is `None`) → titled placeholder.
- **No numeric continuum** (< 2 numerics) → fall back from field to projected
  transition graph; still a phase portrait (arrows = the map's image).
- **Strings / unrepresentable axes** → projected to `0.0` (axis collapses); the
  spiral-spread + collision count keep distinct states visible despite collapse.
- **Infeasible grid points** (`successor` returns `None`) → silently skipped; an
  all-`None` field still emits a titled (empty) figure.
- **Cross-facet edges** → counted and annotated, never drawn (would mislead).
- **Empty / single-point bounds** → padded to a unit window so the panel renders.
- **Enums/bools as axes** → ordinal-encoded with categorical tick labels so the
  axis still reads in the variable's own vocabulary.

---

## 7. Parameters (defaults)

| Parameter | Default | Meaning |
|---|---|---|
| grid size `n` | 21 | points per numeric axis (`n×n` field samples) |
| range probe seeds | `{100, 1000, 3000}` | magnitudes used to auto-scale a numeric axis |
| range factor | 1.4 | window = `±1.4·max|sampled value|` |
| min-magnitude floor | 10 | fallback scale when samples are tiny |
| window floor `span` | 3200 | minimum half-width (so wide oscillators aren't clipped) |
| interior fraction | 0.92 | fixed-point detection excludes the outer 8% border |
| fixed-point ε | 1e-9 | `‖f(p)-p‖` threshold for "no displacement" |
| trajectory steps | 400 | length of each overlaid orbit |
| seeds | 6 | off-origin trajectory start points |
| `reachable` limit | 3000 | BFS node cap for the discrete graph |
| facet max cardinality | ~6 (`facet_var` default) | upper bound on panels |
| facet max change rate | 0.25 (`facet_var` default) | reject vars that vary within a run |
| spiral angle / radius | `2.399963` rad, `0.10 + 0.06k` | golden-angle phyllotaxis declutter |
| panel columns | 3 | facet grid width |

---

## 8. References

- **Strogatz**, *Nonlinear Dynamics and Chaos* — phase portraits, vector fields,
  fixed points, limit cycles; the numeric-regime picture is the discrete-map
  analogue of a continuous flow's phase plane.
- **Hirsch, Smale & Devaney**, *Differential Equations, Dynamical Systems, and an
  Introduction to Chaos* — discrete maps `x_{n+1} = f(x_n)`, orbits, and
  attractors; the displacement field `f(x)-x` and absorbing states.
- **Cleveland & McGill (1984)**, "Graphical Perception" — channel-effectiveness
  ranking (position ≫ length ≫ angle ≫ color/area); justifies axes-for-quantity.
- **Mackinlay (1986)**, "Automating the Design of Graphical Presentations" —
  expressiveness/effectiveness criteria driving the type→channel mapping.
- **Tufte**, *Envisioning Information* — *small multiples*; the basis for faceting
  a categorical into panels instead of overloading one plane.
- **Vogel (1979)** phyllotaxis / golden-angle spiral — the deterministic
  uniform-disk point placement used to declutter colliding projected states.
