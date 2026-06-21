# Time Series

A small-multiples plot of one trajectory of the difference equation: every state
variable drawn against tick number on its own stacked row, all rows sharing the
tick axis.

> Prerequisite reading: **`docs/visualizations/00-core-machinery.md`** — defines the
> transition queries (`initial_state`, `successor`, `successors`, `_key`, `label`),
> the variable ranking/dedup machinery (`state_vars`, `numeric_vars`,
> `categorical_vars`, `enum_variants`, `var_class`), and the channel-fitness model.
> This doc references those primitives by name and does not re-derive them.

---

## 1. What it shows

It answers: **"How does each state variable evolve over time along a single
representative orbit?"** It is the most literal reading of an Evident program as a
difference equation `s_{n+1} = f(s_n)` — pick a start, iterate the map, and plot the
resulting sequence component-by-component.

When to use it:

- **Numeric programs** (int/real phase systems): shows oscillation, decay, growth,
  limit cycles as line plots over tick.
- **Discrete programs** (bool/enum/string state machines): shows the sequence of
  symbolic states as step plots, each row reading as the variable's categorical
  ladder over time.
- **Mixed programs**: numeric and categorical variables share the same tick axis on
  separate stacked rows, so you can read a continuous quantity and a discrete mode
  against the same time index.

It is the universal fallback view: every program has a trajectory, so this renderer
always produces something meaningful, even when phase-space or graph views degrade.

## 2. The object

Let `s_0, s_1, …, s_T` be a finite **orbit** (successor chain) of the transition
map, where each `s_n` is a complete assignment to the state variables. For each state
variable `v` we draw the discrete-time signal

```
n  ↦  s_n[v]          for n = 0 … T
```

as one panel in a vertical stack of panels that share the horizontal (tick) axis:

- **Quantitative `v`** (int/real): a polyline through `(n, s_n[v])` — a sampled
  continuous signal.
- **Categorical `v`** (bool/enum/string): a **post-step** (zero-order-hold) staircase
  through `(n, ord(s_n[v]))`, where `ord` is an ordinal encoding of the symbolic value
  (Section 4, step 5). The panel's y-axis is labelled with the *full* categorical
  ladder of `v`, not just the values the orbit visits, so the row reads as the whole
  domain of the variable.

The full object is the stacked panel array: rows ordered by importance and grouped by
type, columns indexed by tick.

## 3. Inputs

Transition queries (from core machinery):

- `initial_state()` — the first-tick state; the default seed.
- `successor(state)` — one deterministic step; used both to seed (fixed-point
  detection) and as the fallback edge.
- `successors(state)` — the full nondeterministic fan of next states; used by the
  trajectory walk to prefer unvisited successors.
- `_key(state)` — canonical hash of a state, for visited-set membership.
- `label(state)` — short human label of a state, for the title.

Variable info (from core machinery):

- `state_vars` — importance-ranked, deduplicated interface variables.
- `numeric_vars` / `categorical_vars` — the `var_class`-split projections of
  `state_vars`, preserving its order.
- `enum_variants[name]` — ordered variant list for each enum-typed variable.
- `var_class(v)` / `var_class` label — `"quant"` vs `"cat"` classification and the
  per-panel importance badge text.

## 4. Algorithm

### Seed selection — `pick_seed`

1. Let `init = initial_state()`. If `init` is `None` (no first-tick model exists),
   emit a placeholder panel ("no initial state") and stop.
2. Let `numeric = { v ∈ state_vars : kind(v) ∈ {int, real} }`.
3. **Fixed-point detection.** If `numeric` is non-empty, compute `nxt = successor(init)`
   and test whether `init` is a fixed point: `nxt ≠ None ∧ ∀ v ∈ state_vars:
   init[v] = nxt[v]`. Many numeric phase systems have their `initial_state` at the
   origin, which is an equilibrium — iterating it gives a flat, uninformative orbit.
4. **Nudge off the equilibrium.** If `init` is a numeric fixed point, copy it to `seed`
   and offset the *first* numeric axis `v0` by a fixed displacement: `seed[v0] =
   init[v0] + Δ` (default `Δ = 2000`; treat non-numeric/missing as `0` before adding).
   Verify `seed` is a live state (`successor(seed) ≠ None`); if so use it, else fall
   back to `init`. This is **generic** — it keys on "numeric + initial is a self-loop",
   never on a program name.
5. Otherwise the seed is `init`.

### Trajectory walk — `walk`

Iterate the map for at most `STEPS` steps, with a freshness bias so self-loops do not
park the walk immediately:

1. `cur = seed`; `path = [cur]`; `seen = { _key(cur) }`.
2. Repeat up to `STEPS` times:
   a. `nxts = successors(cur)`. If empty, stop (terminal state).
   b. Let `fresh = [ s ∈ nxts : _key(s) ∉ seen ]`. Choose `nxt = fresh[0]` if any fresh
      successor exists, else `nxt = nxts[0]`.
   c. Append `nxt` to `path`.
   d. If `_key(nxt) ∈ seen`, stop — only already-seen / self-loop successors remain, the
      orbit has closed.
   e. Add `_key(nxt)` to `seen`; `cur = nxt`.
3. Return `path = [s_0, …, s_T]` and tick indices `ticks = 0 … T`.

This produces an *exploring* walk on discrete systems with legal self-edges (e.g. an
adjacency graph where staying put is allowed) instead of immediately stopping on a
self-loop, while still terminating once the orbit revisits a state.

### Panel ordering — channel assignment

5. `quant = numeric_vars`; `cat = categorical_vars`. The drawn order is
   `ordered = quant ++ cat` — quantitative rows on top, categorical rows below, each
   group internally in `state_vars` importance order. If `ordered` is empty (no
   classified vars), fall back to `ordered = state_vars`.
6. Stack `len(ordered)` panels vertically, sharing the tick (x) axis. Panel `rank`
   (0-based) gets an importance badge `"#(rank+1)  <var_class label>"`.

### Per-panel rendering

7. **Quantitative panel** (kind int/real): draw the polyline through
   `(n, s_n[name])` for all `n`, with point markers; faint grid. y-axis label = `name`.
8. **Categorical panel** (kind bool/enum/string): ordinal-encode each value (step 9),
   collect `(n, ord)` points, draw a **post-step staircase** (the value at tick `n`
   holds until tick `n+1`). Then build the y-tick ladder:
   - enum: `y = index of value in enum_variants[name]` (default `0` if absent); label
     **every** index `i` with `enum_variants[name][i]` so all variants appear even if
     unvisited.
   - bool: `y ∈ {0,1}` mapped to `{"false","true"}`; always label both.
   - string: `y = 0`, label is the string value (degenerate single-level ladder).

   Set y-limits to `[min(ladder) − 0.4, max(ladder) + 0.4]` so the staircase has margin.

### Ordinal encoding — `to_ordinal`

9. Map a non-numeric value to `(y, label)`:
   - bool: `(1, "true")` if truthy else `(0, "false")`.
   - enum: `(index of value in enum_variants[name], str(value))`, index `0` if not found.
   - string: `(0, str(value))`.

The enum ordering is *whatever order the variants are declared in* — it is a nominal
encoding placed on an integer axis purely for stacking, not an asserted ordinal
relation. Do not read vertical distance between enum levels as magnitude.

## 5. Variable → channel mapping

This view uses only **two channels**, both maximally effective:

- **Tick → x (position).** The shared, universal independent axis. Position is the
  highest-fidelity channel; time is the natural independent variable of a difference
  equation, so it owns x for every panel.
- **Each variable → its own panel's y (position).** Every variable gets a dedicated
  position channel rather than competing for color/size on a single axes. This is the
  small-multiples principle: one quantity per panel, perfectly comparable along a
  common x.

No color or size channel is needed (a fixed hue per type — one for quant lines, one
for categorical steps — is decorative, not encoding). The only real channel decision is
**row order**: most-important variable on top (`state_vars` is already importance-ranked
and deduplicated), and the two `var_class` groups kept contiguous so quantitative lines
and categorical staircases do not interleave. Type-effectiveness reasoning: quantitative
→ position on a continuous line; categorical → position on a labelled ordinal ladder via
a step (zero-order-hold) plot, which is the correct visual idiom for a value that is
constant between transitions.

## 6. Degradation & edge cases

- **No initial state** (`initial_state()` returns `None`): emit a single placeholder
  panel stating the FSM has no first-tick model; produce no trajectory.
- **Numeric fixed-point seed**: the nudge in `pick_seed` (Section 4, steps 3–4) prevents
  a flat orbit. If the nudged point is not a live state, fall back to `init` (accept the
  flat trajectory rather than fabricate one).
- **Immediate terminal / self-loop**: `walk` stops as soon as the orbit revisits a state
  or has no successors; `path` may be as short as one or two states. The plot still
  renders with `T+1` columns.
- **No classified variables**: fall back to plotting raw `state_vars`.
- **Single panel** (`nvars = 1`): treat the lone axes uniformly (wrap in a list so the
  per-panel loop is unconditional).
- **string-typed variables**: collapse to a single-level ladder (`y = 0`); the row shows
  transitions as a flat step plot — informative only that the value changed, not how
  much. Acceptable as a fallback; strings carry little ordinal structure.
- **Mixed programs**: quant rows and cat rows coexist with no special handling — the type
  grouping in step 5 keeps them visually separated.

## 7. Parameters

| Parameter | Default | Meaning |
|---|---|---|
| `STEPS` | `60` | Max trajectory length (ticks iterated). Cap on orbit columns. |
| fixed-point offset `Δ` | `2000` | Displacement added to the first numeric axis to leave a numeric equilibrium seed. |
| successors `limit` | `64` (core default) | Cap on fan size per state when enumerating `successors`. |
| step interpolation | `post` (ZOH) | Categorical staircase holds each value until the next tick. |
| y-tick margin | `±0.4` | Padding above/below the categorical ladder. |
| marker size | `3` | Point markers on lines/steps (cosmetic). |

## 8. References

- **Difference equations / discrete dynamical systems** — Strogatz, *Nonlinear Dynamics
  and Chaos*; the orbit `s_{n+1}=f(s_n)` and time-series of a single trajectory.
- **Small multiples** — Tufte, *The Visual Display of Quantitative Information* and
  *Envisioning Information*: one variable per panel, a shared axis for comparison.
- **Zero-order-hold / step plots for piecewise-constant signals** — standard in
  signal/control visualization; the value is constant between sample instants, so a
  staircase (not a line) is the faithful idiom for categorical state.
- **Stevens' scale typology / channel effectiveness** — Stevens (nominal/ordinal/
  interval/ratio); Cleveland & McGill, *Graphical Perception* (1984); Mackinlay's
  ranking of channels by data type: position is the most accurate channel for both
  quantitative and ordinal data, motivating one-position-channel-per-variable here.
- **Nominal-on-ordinal-axis caveat** — Munzner, *Visualization Analysis and Design*: an
  enum placed on an integer axis is a layout convenience, not an asserted magnitude.
