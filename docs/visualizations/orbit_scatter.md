# orbit_scatter — discrete-time orbit on visual channels

> Reads the shared transition-query and variable-ranking primitives documented in
> [`00-core-machinery.md`](00-core-machinery.md). That file defines
> `initial_state()`, `successor`, `successors`, `trajectory`, `reachable`,
> `state_vars`, `numeric_vars`, `categorical_vars`, `assign_channels`,
> `facet_var`, `enum_variants`, and the `_key` canonicalizer. This document
> specifies ONLY the orbit_scatter algorithm built on top of them.

## 1. What it shows

A **discrete orbit**: the sequence of states an Evident program (a difference
equation `x_{n+1} = f(x_n)`) visits, drawn as **unconnected dots** in a chosen
2-D projection of state space. Each dot is one state at one tick. The crucial
honesty of the view is that **no line connects consecutive states** — the dot
spacing *is* the jump the map makes per step, so the discrete-time nature is not
hidden behind interpolation.

Read the picture by shape:

- A **fixed point** ⇒ dots pile up at one location.
- A **limit cycle** (period-k) ⇒ dots settle onto k recurring positions / a
  closed loop.
- A **strange/continuous attractor** (van der Pol etc.) ⇒ dots fill a closed
  curve from many seeds.
- A **nondeterministic discrete system** ⇒ the reachable state set, dots colored
  by graph distance from the start.

When to use it: any program shape. It degrades gracefully across **numeric**
(continuous-ish 2-D phase plane), **autonomous discrete** (single deterministic
chain), and **mixed / nondeterministic** (reachable cloud). It is the
general-purpose "where does the system go" view; prefer the phase-portrait
vector-field renderer when you want `f(x)-x` displacement arrows rather than
sampled orbits, and the cobweb renderer for 1-D maps.

## 2. The object

Formally, given a projection `π: State → ℝ²` defined by two chosen state
variables `(xvar, yvar)`, the object is the multiset of points

```
P = { π(s) : s ∈ O }
```

where `O` is an **orbit set** — one of:

- **numeric mode**: the union of forward trajectories `{ f^n(s₀) : 0 ≤ n < N }`
  from several hand-placed seeds `s₀` (so a basin/attractor is visible);
- **autonomous mode**: a single forward trajectory from the initial state;
- **reachable mode**: the BFS-reachable state set from the initial state, with
  each state tagged by its minimum step-distance (used when a single chain
  dead-ends, i.e. nondeterministic transition relations).

Each point additionally carries a scalar `t` (tick index in numeric/autonomous
mode; BFS depth in reachable mode) and the originating full state `s` (so color
/ facet channels can read OTHER variables of the same state).

## 3. Inputs (core-machinery primitives consumed)

- `initial_state()` — orbit seed for autonomous / reachable modes.
- `trajectory(start, steps)` — the forward chain `s₀, f(s₀), …`.
- `successors(state)` — fan-out for the reachable-mode BFS.
- `_key(state)` — canonical hashable identity for BFS visited-set dedup.
- `state_vars`, `numeric_vars`, `categorical_vars` — ranked, deduped typed
  interface variables.
- `assign_channels(["x","y"])` — type-effectiveness channel assignment, the
  fallback axis picker.
- `facet_var()` — the guarded low-cardinality, low-change-rate categorical, or
  `None`.
- `enum_variants[name]` — declared variant order for an enum (ordinal encoding
  and tick labels).

## 4. Algorithm

### 4.1 Channel selection — `_select_channels`

Produces `(xvar, yvar, color_var, facet_var)`.

1. **Axes.** If `len(numeric_vars) ≥ 2`, take `xvar, yvar = numeric_vars[0],
   numeric_vars[1]` — a numeric pair is the honest continuous phase plane.
   Otherwise call `assign_channels(["x","y"])`:
   - if it returns no x candidate (no state variables) ⇒ return all-`None`
     (renderer emits an "N/A: no state variables" placeholder);
   - if it returns x but no y (a single state var) ⇒ set `yvar = xvar` (1-D
     system drawn on the diagonal).
2. Let `used = {xvar.name, yvar.name}`.
3. **Facet.** `facet_var = facet_var()` (a categorical that stays ≈constant
   within a run — see core-machinery's change-rate guard). If it is already an
   axis (`name ∈ used`) drop it to `None`; else add its name to `used`.
4. **Color.** First categorical var whose name ∉ `used`; else `None`
   (⇒ the time/depth gradient is used instead).

Rationale: categorical → color/facet, quantitative → position. A categorical on
an axis would force an arbitrary ordinal encoding for a variable that has no
natural order, so categoricals are pushed to hue/panel where nominal data reads
best (Cleveland–McGill / Mackinlay effectiveness ranking).

### 4.2 Orbit construction — `_build_orbits`

Let `numeric_2d = (xvar.kind ∈ {int,real}) ∧ (yvar.kind ∈ {int,real}) ∧
(xvar.name ≠ yvar.name)`.

1. **If `numeric_2d`** → mode `"numeric"`. For each seed (§4.3) call
   `trajectory(start=seed, steps=N)`; keep non-empty orbits; `t` for each = its
   tick index `0..len-1`.
2. **Else** compute `orb = trajectory(initial_state(), steps=N)`.
   - If `len(orb) > 2` → mode `"autonomous"`, single orbit, `t` = tick index.
   - Else the chain dead-ended ⇒ run **reachable-with-depth BFS** (§4.4); mode
     `"reachable"`, single "orbit" = the reachable state list, `t` = BFS depth.
3. If nothing was produced ⇒ renderer emits an "N/A: no orbit produced"
   placeholder.

### 4.3 Numeric seeds — `_numeric_seeds`

A small fixed set of initial `(x, y)` points chosen to fall in different regions
so that a basin/attractor is revealed rather than one orbit. Each partial seed is
completed to a full state by setting every other state var to `0`. (Defaults are
tuned for the van der Pol fixed-point scale; see §7. A reimplementation should
derive seeds from observed variable ranges rather than hardcoding, e.g. sample
the corners and center of a bounding box.)

### 4.4 Reachable-with-depth BFS — `_reachable_with_depth`

Standard breadth-first search over the transition graph, recording shortest
step-distance:

```
init ← initial_state();  if None → ∅
states ← [init];  index ← { _key(init): 0 };  depth ← [0];  frontier ← [0]
while frontier ≠ ∅ and |states| < LIMIT:
    i ← pop_front(frontier)
    for nxt in successors(states[i]):
        k ← _key(nxt)
        if k ∉ index:
            index[k] ← |states|;  append nxt to states
            append depth[i]+1 to depth;  push index[k] to frontier
return states, depth
```

`depth[i]` is the geodesic distance (in transition steps) from the initial
state — the BFS layer.

### 4.5 Point flattening and projection

For every orbit `oi` and every state `st` at position `ti`, emit a point with:

- `x = π_x(st) = _project(xvar, st[xvar.name])`,
- `y = π_y(st) = _project(yvar, st[yvar.name])`,
- `t = point_time[oi][ti]` (tick or BFS depth),
- `seed = oi`, `first = (ti == 0)`, and the full state `st`.

**Projection `_project(var, value) → ℝ`** (the ordinal encoding):

| kind | mapping |
|---|---|
| int / real | `float(value)` (pass-through) |
| bool | `true → 1.0`, `false → 0.0` |
| enum | index of the value in `enum_variants[var.name]` (declared order) |
| other | `0.0` |

### 4.6 Discrete jitter

Let `discrete = ¬(xvar.kind ∈ {int,real} ∧ yvar.kind ∈ {int,real})`. When an
axis is bool/enum, many states project to the SAME lattice point and would
overplot. Apply a small deterministic jitter so coincident dots separate without
crossing the integer grid:

```
h ← (hash(round(x,3), round(y,3), t, seed) mod 2^16) / 2^16    # in [0,1)
x ← x + 0.11·sin(2π·h + 0.7·t)
y ← y + 0.11·cos(2π·h + 0.7·t)
```

Amplitude `0.11 < 0.5` keeps every dot nearer its own lattice cell than any
neighbor; the hash makes it stable across renders; the `t` term spreads a long
dwell at one state into a visible cluster. (A reimplementation may substitute any
stable hash → unit-disk perturbation with amplitude < half the lattice spacing.)

### 4.7 Faceting (small multiples)

If `facet_var ≠ None`: the panel values are `enum_variants[facet_var.name]`
(enum) or `[false, true]` (bool), intersected with the values that actually
occur among the points. One panel per present value, sharing x/y scales. Each
panel draws only the points whose `st[facet_var.name]` equals that panel's value.
Otherwise a single panel.

Faceting is the honest way to add a dimension for a high-D model: a panel is one
slice at a fixed value of a configuration/regime variable that does not move
along the trajectory. Faceting by a variable that *changes* on the orbit would
shred the dynamics across panels — which is exactly why `facet_var()` only
returns low-change-rate categoricals.

### 4.8 Color

- **Categorical color var present** → nominal palette (one distinct hue per
  value of `enum_variants[color_var.name]` or `[false, true]`); each point colored
  by `st[color_var.name]`; a legend maps hue → category label. Use a qualitative
  (max-distinct-hue) palette.
- **No categorical free** → sequential **time/depth gradient**: color by `t`,
  normalized `0 … max_t`, with a perceptually-uniform sequential colormap
  (viridis). This is the one strong quantitative use of color — an ordered
  gradient — and labels as "tick (time)" (numeric/autonomous) or "steps from
  start" (reachable).

### 4.9 Marking and labeling

- **Seeds/starts**: every point with `first = true` gets a hollow ring overlay so
  initial conditions are identifiable (multiple in numeric mode).
- **Axis ticks**: for an enum axis, place ticks at `0..k-1` labeled with the
  variant names; for a bool axis, ticks at `{0,1}` labeled `false`/`true`;
  numeric axes use ordinary scales. (This is where the ordinal projection is made
  legible — the viewer never sees raw integer codes for nominal values.)

## 5. Variable → channel mapping

| channel | gets | rule / reasoning |
|---|---|---|
| **x, y (position)** | top-2 `numeric_vars`, else `assign_channels` best pair | Position is the highest-accuracy quantitative channel; numeric pair = the true phase plane. Nominal vars only land here as a last resort (single-var systems), via ordinal encoding. |
| **color** | first free categorical, else `t` gradient | Nominal → distinct hues (best categorical channel); when none free, an *ordered* time/depth scalar → sequential colormap (the legitimate quantitative use of color). |
| **facet** | `facet_var()`: low-card (≤ ~5–6), low-change categorical not on an axis | Small multiples add a dimension without distorting the 2-D geometry; must be ≈constant per run. |
| **size** | (designed slot) a secondary numeric var | Quantitative, lower-accuracy than position — reserved for a free numeric var. (Not wired into the current axis-driven plot; the plot must be readable from axes alone, with color/facet/size only enhancing.) |

Effectiveness ordering follows Cleveland–McGill (position ≫ length/area ≫ color)
and Mackinlay's automatic-presentation rankings (quantitative→position,
nominal→color/shape/panel).

## 6. Degradation & edge cases

- **No state variables** → `assign_channels` yields no x ⇒ "N/A for state: no
  state variables" placeholder.
- **Single state variable** → `yvar = xvar`; the orbit collapses onto the
  diagonal (still shows dwell/cycle structure in one dimension).
- **Pure numeric (≥2 numeric vars)** → numeric mode, multi-seed trajectories, no
  free categorical ⇒ time-gradient color.
- **Discrete deterministic (autonomous)** → single chain; enum/bool axes ⇒
  ordinal projection + jitter; categorical color/facet if available.
- **Nondeterministic / dead-ending chain** (`len(trajectory) ≤ 2`) → reachable
  BFS cloud colored by graph depth.
- **No initial state and no successor** → "N/A: no orbit produced" placeholder.
- **Facet/color collision**: a var already used as an axis is never reused for
  facet or color (`used` set guard).
- **Enum/bool overplot**: handled by §4.6 jitter; panel value lists are pruned to
  values that actually occur.

## 7. Parameters (defaults)

| name | default | meaning |
|---|---|---|
| trajectory steps `N` | `400` | forward orbit length per seed / autonomous chain |
| BFS `limit` | `400` | max reachable states enumerated (reachable mode) |
| `successors` per-state cap | `64` (core-machinery default) | fan-out bound per node |
| numeric seeds | 4 fixed `(x,y)` points (others = 0) | basin coverage; reimplement as bbox corners+center |
| jitter amplitude | `0.11` (< 0.5 lattice) | separates coincident discrete dots |
| facet max cardinality | ~`5–6` | panel-count ceiling (from `facet_var()`) |
| facet max change-rate | `0.25` | "≈constant within a run" guard (from `facet_var()`) |
| color palettes | qualitative (categorical) / viridis (gradient) | nominal vs sequential |

## 8. References

- W. S. Cleveland & R. McGill, *Graphical Perception* (1984) — accuracy ordering
  of visual channels (position ≫ length ≫ angle ≫ color/area).
- J. Mackinlay, *Automating the Design of Graphical Presentations* (APT, 1986) —
  quantitative→position, nominal→color/shape/panel effectiveness rankings.
- E. R. Tufte, *The Visual Display of Quantitative Information* — small multiples
  (faceting) as a way to add a dimension without distorting the base geometry.
- Strogatz, *Nonlinear Dynamics and Chaos*; Hirsch–Smale–Devaney — orbits, fixed
  points, limit cycles, and the discrete iterated-map / phase-plane reading the
  scatter encodes (van der Pol oscillator as the canonical limit-cycle example).
- BFS shortest-path layering (Cormen et al., *Introduction to Algorithms*) — the
  reachable-mode depth tagging.
- C. Ware, *Information Visualization: Perception for Design* — sequential vs
  categorical colormap choice; perceptual uniformity (viridis).
