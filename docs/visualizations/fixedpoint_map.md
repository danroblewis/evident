# Fixed-Point Map

A language-agnostic specification of the `fixedpoint_map` visualization for
Evident programs. Reimplementable from this doc plus
[`00-core-machinery.md`](00-core-machinery.md) (the shared transition-query and
variable-ranking layer) and any SMT solver + 2-D drawing surface.

---

## 1. What it shows

The fixed-point map answers: **"where does the system come to rest, and what
does it circle?"** It locates the *attractors* of the difference equation — the
states the dynamics converge onto — and draws them against the cloud of all
sampled states so they stand out against their basins.

Two kinds of attractor are surfaced:

- **Fixed points** — states `s` that are *absorbing*: every successor of `s` is
  (approximately) `s` itself. The system, once there, never leaves.
- **Short cycles / limit cycles** — successor chains
  `s → s₁ → … → s` of period ≥ 2 that return to their start. These are
  periodic orbits (discrete short cycles) or continuous limit cycles (a numeric
  orbit the trajectories spiral onto).

**When to use it.** Any program whose long-run behavior is the question:
- *Numeric* systems with a continuous phase plane (oscillators, e.g. a Van der
  Pol limit cycle) — the orbit only exists in a continuous 2-D plane.
- *Discrete* systems (enum/bool/string state) whose reachable graph has
  absorbing sinks or short loops (a vending machine returning to idle, a
  game-state graph with terminal states).
- *Mixed* systems — numeric axes scanned, discrete axes pinned per slice.

It is the complement of a phase portrait: a phase portrait shows the *flow*;
this shows the flow's *ω-limit sets* (its destinations) explicitly classified.

---

## 2. The object

A 2-axis projection of the state space onto coordinates `(x, y)`, on which:

- every **sampled state** is a faint background dot (the basin),
- every **fixed point** is a large filled star marker,
- every **cycle** is its member states linked head-to-tail by directed arrows
  around the loop (short cycles) or a continuous polyline with periodic arrow
  glyphs (long limit cycles),

optionally split into **facet panels** by a configuration variable, and with the
background basin optionally tinted by a categorical **color** / **shape**
channel. The derived attractor coloring (fixed = red, cycle = blue) always draws
on top and is never overridden by the categorical channels.

Mathematically: given the transition relation `T ⊆ S × S` and its induced map
`f(s) = successor(s)`, the object renders
- `Fix = { s ∈ sampled : ∀ t. (s,t) ∈ T ⇒ t ≈ s }` (absorbing states), and
- a set of closed orbits `{ (s₀, …, sₖ, s₀) : f^{i}(s₀) ≈ s₀ for the return }`,
projected via an ordinal embedding `π : S → ℝ²`.

---

## 3. Inputs

From the core-machinery API (see `00-core-machinery.md` for each):

- `initial_state()` — first-tick state; seeds the numeric grid's auto-range.
- `reachable(limit)` — BFS reachable state set (preferred sample source).
- `successor(state)` — one step of the map `f`; drives chain-following.
- `successors(state, limit)` — *all* next states; drives the absorbing test
  (a state is at rest only if **every** successor maps back).
- `state_vars` — ranked, deduped interface variables `{name, prev, kind}`.
- `numeric_vars` / `categorical_vars` — type-partitioned ranked vars.
- `facet_var()` — a near-constant categorical config var, or `None`.
- `enum_variants[name]` — ordered variant list (for ordinal encoding + ticks).
- `_key(state)` — canonical hash key for set membership / dedup.

This viz consumes the transition queries directly; it does **not** use the
mRMR channel-fitting (`assign_channels`) for its *axes* — axes are chosen by the
local rule in §5. It does reuse `facet_var()` and the numeric/categorical
partition.

---

## 4. Algorithm

### 4.1 Sample the state space → `(states, mode)`

The reachable set *is* the real dynamics, so prefer it; fall back to a phase-grid
only when the reachable set collapses and the system is numeric.

1. `reach, _ = reachable(limit=5000)`.
2. `has_numeric = any var.kind ∈ {int, real}`.
3. If `|reach| > 2`, or (`reach` non-empty **and** not numeric): use
   `(reach, "reachable")`. (Discrete systems and rich numeric graphs.)
4. Else if `has_numeric`: build a **grid scan** (§4.2), then union in the
   reachable point(s) (so the true fixed point at the origin is kept). Return
   `(grid, "grid")`. This is the Van der Pol case: reachable collapses to the
   origin alone, so we scan the surrounding phase box to expose the orbit.
5. Else return `(reach, "reachable")`.

### 4.2 Grid scan (numeric / mixed) — `grid_states`

Partition `state_vars` into numeric (`int/real`) and discrete (`bool/enum/string`).

1. **Discrete combinations.** Cartesian-product the discrete domains
   (`bool → {false,true}`, `enum → variants`). Cap the product at **64** combos
   (truncate if it explodes).
2. **Numeric axis ranges.** For each numeric var, build a uniform 1-D grid over a
   symmetric box `[−base, +base]` (§7 for `base` heuristic), with `per_axis`
   points where
   `per_axis = clamp( floor( (max_points / |disc_combos|)^{1/|numeric|} ), 2, 40 )`
   so the total point budget (`max_points = 900`) is respected.
3. **Product.** For each discrete combo, take the full Cartesian product of the
   numeric axis grids; for `int` vars cast the grid sample to integer. Append
   states until `max_points` is reached.

For a purely-discrete system this returns the discrete combos themselves
(though §4.1 prefers the exact reachable set there).

### 4.3 Detect attractors — `find_attractors(states, mode)`

Tolerances depend on `mode` (a coarse grid needs slack to close orbits; an exact
reachable scan uses equality). With `mode == "grid"`:
`cyc_tol = 30`, `fix_tol = 1`, `max_len = 360`. With `mode == "reachable"`:
`cyc_tol = fix_tol = 0` (exact), `max_len = 40`.

**Approximate equality** `near(a, b, tol)`: exact equality on every discrete axis;
on each numeric axis require `|aᵢ − bᵢ| ≤ tol`. (A fixed point must truly not
move, so `fix_tol` is always tight; cycle-closing gets the looser `cyc_tol`.)

**Fixed points.** For every sampled state `s`, call `successors(s, limit=8)`
(falling back to the single `successor` if the set query is unavailable). `s` is
absorbing iff the successor set is non-empty and `near(s, t, fix_tol)` for
**all** `t`. A self-loop that *also* has other exits is **not** at rest. Collect
all absorbing `s` into `Fix`.

**Cycle seeds.** Probing every grid point for a long chain is expensive, so:
- `mode == "grid"`: use a spread of **numeric seeds** (§4.4).
- `mode == "reachable"`: use all sampled states.

**Cycle following** `find_cycle_from(s)`: follow the deterministic chain
`s, f(s), f²(s), …` up to `max_len`. At each new node, if it lands `near` an
earlier chain node `chain[j]`, close the loop `chain[j:] + [chain[j]]` and return
it if its length ≥ 3 (period ≥ 2). Skip any seed already `near` a fixed point.

**Cycle dedup.** Key each cycle by the *set* of its members, coarsened: numeric
axes quantized to `round(value / max(cyc_tol, 1))`, discrete axes verbatim. Drop
cycles whose member-set key was already seen.

### 4.4 Numeric seeds — `pick_numeric_seeds`

For systems with ≥ 2 numeric axes, seed a **polar ring** in the (x, y) plane:
radii `r ∈ {0.15, 0.5, 0.85} · base` × 8 angles `k·2π/8`, where `base` is the
max observed |x| over the sampled states (or 3000). The 0.5/0.85 rings land in a
limit-cycle basin; the 0.15 ring catches a central fixed point's basin. Each
seed copies a template discrete combo and overwrites only the two numeric axes.
(With < 2 numeric axes, fall back to the first ~60 sampled states.)

### 4.5 Long limit cycle extraction (grid, no cycle found) — `extract_limit_cycle`

Spiral-onto-a-cycle orbits (Van der Pol) take a long transient + full period,
too long for the per-seed chain above. So when `mode == "grid"` and no cycle was
found:

1. From each seed not near a fixed point, run a **long trajectory** of up to
   ~700 steps via `successor`.
2. Discard chains shorter than 200 (didn't run long enough).
3. **Drop the transient**: take the *settled tail*, `tail_start = ⌊0.45·len⌋`.
4. **Near-recurrence search**: scanning the tail, find the latest pair of indices
   `j < i` (each at least ~30 apart, `i` after `tail_start+30`) with Euclidean
   numeric distance `‖chainᵢ − chainⱼ‖₂ ≤ 40`. That pair brackets one closed
   period; return `chain[j:i] + [chain[j]]` if length ≥ 4.

This recovers the limit cycle as the geometric loop the settled orbit traces.

### 4.6 Faceting & rendering

1. Choose channels (§5). If a facet var was chosen, panels = its domain;
   else a single panel.
2. Attractors are a **global** property — detect them once (§4.3), then route
   each member into the facet panel it belongs to. A cycle belongs to panel
   `pval` iff *all* its members share that facet value (a discrete facet axis
   doesn't change along a numeric limit cycle).
3. Per panel, with projection `π(s) = (ordinal(x, s.x), ordinal(y, s.y))`:
   - draw background sampled states (faint; or per categorical color/shape cell);
   - draw cycles: long orbits (> 12 members) as a polyline with ~8 evenly-spaced
     arrow glyphs; short cycles as arrow-linked member markers;
   - draw fixed points as large red star markers on top.
4. Lay panels in a grid ≤ 3 columns. Title each with `facet = value`; super-title
   reports scan mode, channels, and the attractor census (counts + periods).

**Ordinal embedding** `ordinal(var, value)` (used for every plotted coordinate):
`int/real → float(value)`; `bool → {false↦0, true↦1}`;
`enum → index in enum_variants`; `string → hash(value) mod 997` (a stable but
arbitrary spread). This is the projection `π` from `S` to `ℝ²`.

---

## 5. Variable → channel mapping

Axes are chosen for **geometric faithfulness**, not generic channel fit: a limit
cycle only lives in a continuous phase plane, so numeric vars claim x/y first.

| numeric_vars | x | y | rationale |
|---|---|---|---|
| ≥ 2 | `numeric[0]` | `numeric[1]` | continuous phase plane (orbits need it) |
| 1 | `numeric[0]` | top categorical (ordinalized) | mixed: one continuous axis + one ordinal |
| 0 | top categorical | next categorical | purely discrete ordinal projection |

After axes, the **facet** channel is claimed *before* secondary channels, from
`facet_var()` (a near-constant config/regime var), so a good facet var isn't
stolen by color. Remaining categoricals fill secondary channels by
type-effectiveness order: **color (hue)** first, then **shape (marker glyph)** —
both excellent encodings for nominal data, applied only to the *background
basin*.

Type-effectiveness reasoning: quantitative variables → spatial **position**
(x/y), the most accurate visual channel, because orbit geometry is metric;
categorical variables → **color / shape / facet**, where hue and glyph separate
classes without implying a magnitude. The derived 2-class attractor encoding
(red fixed vs. blue cycle) overrides everything and is drawn last.

---

## 6. Degradation & edge cases

- **No states sampled, or no x-axis assignable** → placeholder panel
  ("no states could be sampled from the transition").
- **Purely discrete** → reachable set is the sample (exact equality, no grid);
  fixed points and short cycles found by exact chain-following. Enum/bool axes
  ordinalized with categorical tick labels.
- **Mixed** → grid-scan numeric axes, enumerate discrete combos (capped at 64).
  The second axis is the top categorical, ordinalized.
- **Numeric, reachable collapses to a point/pair** → fall back to phase-grid scan
  (§4.1.4) and union the true fixed point back in.
- **No attractors found** → super-title states "no fixed points / short cycles
  found"; only the basin renders.
- **Spiral limit cycles** that never close within `max_len` → the dedicated
  long-trajectory tail extraction (§4.5).
- **Discrete-combo explosion** (> 64) or **point budget** (> 900) → truncated,
  trading completeness for a bounded scan.
- **String axes** → hashed ordinal; positions are arbitrary but stable, so
  clustering is meaningless on a string axis (treat as a nominal scatter).

---

## 7. Parameters

| Parameter | Default | Meaning |
|---|---|---|
| `reachable` limit | 5000 | BFS cap on the reachable sample |
| `max_points` (grid) | 900 | total grid-scan state budget |
| per-axis grid points | clamp(·, 2, 40) | resolution split across numeric axes |
| discrete combo cap | 64 | max enumerated discrete combinations |
| numeric box `base` | 3200, or `1.2·|init|` if larger | symmetric grid half-width |
| `fix_tol` | 1 (grid) / 0 (reachable) | absorbing-state numeric tolerance |
| `cyc_tol` | 30 (grid) / 0 (reachable) | cycle-closing numeric tolerance |
| `max_len` (chain) | 360 (grid) / 40 (reachable) | per-seed chain length bound |
| `successors` limit (absorbing test) | 8 | successor fan cap |
| seed rings × angles | radii {0.15,0.5,0.85}·base × 8 | polar seed spread |
| long-trajectory length | 700 | settling run for limit-cycle extraction |
| min settled length | 200 | reject too-short trajectories |
| transient drop | `0.45·len` | tail start for recurrence search |
| recurrence threshold | 40 (Euclidean) | near-return distance to close a loop |
| string hash modulus | 997 | ordinal spread for string axes |
| min cycle period | 2 | loops shorter are not cycles |
| facet panel columns | ≤ 3 | panel grid width |

These defaults are tuned for fixed-point ints scaled to ~±3000 (the Van der Pol
sample). A reimplementation should expose `base`, `max_points`, and the
tolerances; the rest are robust.

---

## 8. References

- **Fixed points & ω-limit sets.** Strogatz, *Nonlinear Dynamics and Chaos*
  (2015) — attractors, basins of attraction, limit cycles, stability of
  equilibria. The viz literally renders ω-limit sets of the induced map.
- **Limit cycles & the Van der Pol oscillator.** Van der Pol (1926); Strogatz
  ch. 7. The spiral-onto-orbit transient/tail extraction (§4.5) is the discrete
  analogue of finding the stable limit cycle by integrating and dropping the
  transient.
- **Poincaré recurrence / near-return detection.** The "tail returns near an
  earlier tail point" closure test is a Poincaré-style return criterion in the
  embedding space.
- **Iterated maps & periodic orbits.** Devaney, *An Introduction to Chaotic
  Dynamical Systems* — periodic points of `fⁿ`, which the chain-following
  procedure detects directly.
- **Reachability / BFS on transition systems.** Standard model-checking state-
  space exploration (Clarke, Grumberg, Peled, *Model Checking*).
- **Visual channel effectiveness.** Cleveland & McGill (1984), *Graphical
  Perception*; Mackinlay (1986), *Automating the Design of Graphical
  Presentations* — the ranking position > color/shape that drives the
  quantitative-to-axis, categorical-to-hue/glyph mapping in §5.
- **Phase-plane / vector-field portraits.** The basin-plus-attractor layout
  follows the standard phase-portrait convention (trajectories + marked
  equilibria) from the dynamical-systems literature above.
