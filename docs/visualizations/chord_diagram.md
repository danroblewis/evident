# Chord / Arc Diagram

Render method: `chord_diagram`. Reimplementable spec. Shared primitives
(`reachable`, `successor`, `initial_state`, `categorical_vars`, `numeric_vars`,
`enum_variants`, `state_vars`, variable ranking) are defined in
[`00-core-machinery.md`](00-core-machinery.md); this doc only specifies what is
unique to the chord diagram.

## 1. What it shows

A chord diagram answers: **for one categorical state variable, how does flow
move between its values under the transition relation?** The nodes are the
distinct values of that variable arranged on a circle; a directed arc from value
`a` to value `b` means "some reachable transition took the variable from `a` to
`b`," and the arc's thickness encodes how many transitions did so.

It is the right picture when the program has at least one **categorical** state
variable (enum / bool / string) whose value-to-value transition structure is the
story — a finite-state controller, a mode machine, a room/location graph, a
protocol. It collapses the full reachable state graph down to a single variable's
self-flow, which is exactly what you want when that variable is the "what state
am I in" axis and the rest of the state is detail.

For a **pure-numeric** program (no categorical variable at all) it degrades
gracefully: it bins the top numeric variable into ordinal bands and shows
band-to-band flow (Section 6).

## 2. The object

Formally, the diagram is a drawing of a **weighted directed multigraph
quotient**. Let `π : S → L` be the projection of a full state `s` onto the value
of the chosen node variable (`L` = the set of value-labels). Given the reachable
transition edge set `E ⊆ S × S`, form the **quotient / contracted graph** on `L`:

```
  flow(a, b) = | { (s, s') ∈ E : π(s) = a ∧ π(s') = b } |
```

`flow` is a function `L × L → ℕ`. Self-edges (`a = b`, the variable unchanged
across a transition) are kept and drawn as loop petals. The rendered object is:

- **Nodes**: the labels `L`, placed at equal angular spacing on a circle.
- **Arcs**: one directed curve per nonzero `flow(a,b)`, a quadratic Bézier bowed
  toward the circle centre, with an arrowhead at the destination.
- **A second categorical variable**, if present, hues each arc (Section 5).

This is the standard chord/arc-diagram idiom (Cleveland–McGill: a single
categorical axis bent into a ring; flow drawn as chords across the interior).

## 3. Inputs

From the core machinery:

- `categorical_vars`, `numeric_vars`, `state_vars` — ranked variable lists used
  to pick the node variable and the color variable.
- `enum_variants[name]` — the label set `L` for an enum node/color variable.
- `reachable()` → `(states, edges)` — the reachable state list and the edge set
  `E` (as index pairs into `states`). This is the sole transition source in
  categorical mode.
- `successor(state)` — one forward step of the map; the transition source in the
  numeric-fallback grid sweep.
- `initial_state()` — used by `numeric_range` to seed the band extent.

No transition data is ever hardcoded; every edge comes from solving the
transition relation.

## 4. Algorithm

### 4.1 Pick the node variable (the position axis)

1. If `categorical_vars` is nonempty, the node variable `v` is
   `categorical_vars[0]` (the top-ranked categorical). Determine its label set
   and projection `π`:
   - **enum**: `L = enum_variants[v]`; `π(s) = s[v]`.
   - **bool**: `L = ["false","true"]`; `π(s) = "true" if s[v] else "false"`.
   - **string**: labels are *not* known a priori — `L` is discovered as the
     distinct observed values of `s[v]` over `states` (in first-seen order); `π(s)
     = s[v]`.
2. If there is no categorical variable, set mode = numeric and let `v =
   numeric_vars[0]`; labels are resolved after binning (Section 6).

### 4.2 Pick the color variable (optional second categorical)

Scan `categorical_vars` for the first variable whose name differs from the node
variable. If found, it is the **color variable** with its own labels and
projection (same enum/bool/string resolution as above). If none exists, arcs use
a derived weight gradient instead (Section 5).

### 4.3 Gather flow — categorical mode

1. Call `reachable()` → `(states, edges)`.
2. For string node/color variables, scan `states` once to materialize the
   first-seen distinct-value label lists.
3. For each edge `(i, j) ∈ edges`:
   - `a = π(states[i])`, `b = π(states[j])`.
   - Increment `flow[(a,b)]`.
   - If a color variable exists, cast a **vote** for the color label
     `cproj(states[j])` (the *destination's* color value) in a per-arc tally
     `cat_votes[(a,b)]`.
4. Resolve each arc's color category by **majority vote**:
   `arc_cat[(a,b)] = argmax_label cat_votes[(a,b)]`. (A given `a→b` arc may be
   realized by many transitions whose destination color value differs; the
   majority is the representative hue.)

### 4.4 Layout

Let `n = |L|`. Place label `i` at angle

```
  θ_i = π/2 − 2π·i / n          (start at top, proceed clockwise)
```

on the unit circle: `pos(label_i) = (R cos θ_i, R sin θ_i)`, `R = 1`.

### 4.5 Arc geometry

For each `(a,b)` with weight `w = flow(a,b)`, let `wmax = max flow` and
`frac = w / wmax`.

- **Width**: `lw = 0.8 + 6.5·frac` (linear in normalized weight).
- **Opacity**: with categorical color, `α = 0.55 + 0.40·frac`; with the weight
  gradient, `α = 0.30 + 0.60·frac` (heavier arcs are more opaque).
- **Non-self arc** (`a ≠ b`): a quadratic Bézier with endpoints `pos(a)`, `pos(b)`
  and a single control point pulled toward the centre:
  `ctrl = (0.18·(x0+x1), 0.18·(y0+y1))`. The 0.18 factor bows the chord inward so
  parallel chords fan rather than overlap as straight secants. An arrowhead
  triangle is drawn near the destination, oriented along the tangent
  `ctrl → end`, backed off the node by `0.07` and sized `0.035 + 0.03·frac`.
- **Self-loop** (`a = b`): a small cubic-Bézier petal pointing **radially
  outward** from the node — two control points offset by `0.16` along the outward
  unit normal `(x/r, y/r)` and its perpendicular, starting and ending at the node.

Draw arcs **sorted ascending by weight** so heavy arcs paint last (on top).

### 4.6 Nodes and labels

- **Outgoing total**: `out_tot(a) = Σ_b flow(a,b)`. **Node radius** scales with
  it: `sz = 0.04 + 0.06·(out_tot(a)/max_node)` where `max_node = max out_tot`.
  Draw each node as a filled disc with a white outline.
- **Label**: placed just outside the ring at radius `1.18` along the node's
  angle. Horizontal anchoring follows `cos θ`: right of vertical → left-anchored,
  left → right-anchored, near-vertical → centered.

## 5. Variable → channel mapping

| Channel | Variable / quantity | Reasoning |
|---|---|---|
| **Node position (the ring)** | `categorical_vars[0]` | A chord ring is a single categorical axis; position is wasted on quantitative data here, so the top categorical owns it. |
| **Arc hue (color)** | second categorical var (destination's value), majority-voted per arc | Color is highly effective for nominal data; a low-cardinality second enum/bool rides color to add a dimension ("does this move leave you *escaped* / *dispensing*?"). |
| **Arc width + opacity (size)** | `flow(a,b)` transition count (derived) | A genuinely quantitative derived measure — magnitude of flow — belongs on the size/saturation channel. |
| **Node size** | `out_tot(a)` outgoing flow total (derived) | Quantitative; sizes the source's overall activity. |

When **no** second categorical variable exists, the hue channel falls back to a
**sequential weight gradient**: `color = viridis(0.15 + 0.8·frac)`, mapping
transition count to hue/lightness. This keeps the size information legible even
without a nominal color variable, at the cost of one redundant encoding (width
and hue both track weight).

## 6. Degradation & edge cases

- **Has ≥1 categorical var (enum/bool/string)** — primary mode (Section 4.3).
  Bool gives a 2-node ring; string discovers its node set dynamically.
- **Has a second categorical var** — color channel activates (per-arc majority
  hue + a discrete legend).
- **Pure numeric (no categorical var)** — fallback:
  1. Establish a range `[lo, hi]` for `v = numeric_vars[0]` via `numeric_range`:
     probe `initial_state()` and up to 200 reachable states; if the observed
     spread `> 1`, use a symmetric span `±max(1.2·max|value|, 1)` (widened 20% so
     a limit cycle is not clipped at the seed); else fall back to `±3200` (a
     fixed-point initial state with no spread, e.g. a Van der Pol seed).
  2. Partition `[lo, hi]` into `nbins = 8` equal bands; bin centers become the
     labels (formatted `±N` or `±N.Nk` for |center| ≥ 1000). `to_bin(val)` uses a
     right-side binary search clamped to `[0, nbins−1]`.
  3. **Grid sweep**: build a coarse seed grid over the leading ≤2 numeric vars,
     11 points each across each var's own range. Every other numeric var is
     pinned to 0; bools to false; enums to their first variant.
  4. For each seed `s`, take `successor(s)`; if non-null, increment
     `flow[(to_bin(s[v]), to_bin(next[v]))]`. The result is a band→band flow ring,
     colored by the weight gradient.
- **No values for the primary var** (`n = 0`) — emit a placeholder panel ("N/A
  for this state: no values for primary var").
- **Any render exception** — caught and replaced by a placeholder panel naming
  the error, so the diagram never aborts the batch.

## 7. Parameters

| Name | Default | Meaning |
|---|---|---|
| `nbins` | 8 | numeric-fallback band count |
| grid points per axis | 11 | seed samples per leading numeric var |
| grid axes | ≤ 2 | number of numeric vars swept (others pinned to 0) |
| `reachable(limit)` (range probe) | 200 | states sampled to estimate numeric range |
| range widen factor | 1.2 | span multiplier so limit cycles aren't clipped |
| fixed-point fallback span | ±3200 | when observed spread ≤ 1 |
| arc width | `0.8 + 6.5·frac` | linear in normalized weight |
| arc opacity | `0.55+0.40·frac` (cat) / `0.30+0.60·frac` (grad) | per color mode |
| Bézier control pull | 0.18 | inward bow of chords toward centre |
| node radius | `0.04 + 0.06·(out_tot/max_node)` | sized by outgoing flow |
| label radius | 1.18 | label placement outside ring |
| categorical palette | `tab10` | qualitative, ≤10 hues (wrapped mod 10) |
| gradient palette | `viridis` | sequential, perceptually uniform |

## 8. References

- **W. S. Cleveland & R. McGill**, *Graphical Perception* (1984) — the
  channel-effectiveness ranking (position > length > angle > area > color) behind
  putting the categorical axis on position and weight on size/saturation.
- **J. Mackinlay**, *Automating the Design of Graphical Presentations* (APT,
  1986) — type-driven channel assignment (quantitative → position/length,
  nominal → color/shape).
- **M. Krzywinski et al.**, *Circos* (Genome Research, 2009) — the canonical
  circular ribbon/chord layout for flow between categories.
- **Graph contraction / quotient graph** — the `flow` map is the edge-weighted
  contraction of the reachable transition graph along the fibers of `π`; standard
  directed-graph quotient (see any graph-theory text, e.g. Diestel).
- **Sequential vs. qualitative colormaps** — C. Brewer, *ColorBrewer* — sequential
  (`viridis`) for the derived weight gradient, qualitative (`tab10`) for the
  nominal color variable.
