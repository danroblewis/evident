# Timing Diagram (Waveform Trace)

A single-trajectory, multi-track waveform plot — the EE/logic-analyzer view of an
Evident difference equation. Every state variable becomes one horizontal track
plotted against tick number; tracks are stacked in importance order. The encoding
of each track is keyed to the variable's *type*: digital (held flat, jumps on
vertical edges) for discrete vars, analog (continuous line) for numeric vars.

> Shared primitives — transition queries (`successor`, `successors`,
> `initial_state`, `is_discrete`), variable ranking/dedup (`state_vars`),
> `enum_variants`, `label`, and the `_key` state hash — are defined in
> [`00-core-machinery.md`](00-core-machinery.md). This document specifies only
> what the timing diagram does on top of them.

---

## 1. What it shows

The question: **how does each state variable evolve over time along one execution
path?** It answers "what does this program *do*, step by step" — the temporal
trace, not the global phase structure.

Where a phase portrait shows the *geometry* of the map over its state space, the
timing diagram shows *one trajectory unrolled in time*, with every variable
visible simultaneously on its own track so correlations across variables ("when
`mode` flips to `Pour`, `coins` drops to 0") are read off vertically.

When to use it:

- **Any program shape** — numeric, discrete, or mixed. Unlike a 2-D phase
  portrait it does not require two suitable axis variables; it scales to *N*
  variables by stacking *N* tracks.
- Especially good for **mixed and high-dimensional** systems where no single 2-D
  projection is faithful: a vending machine (enum mode + int coins + bool
  dispensing), a protocol state machine, a counter with flags.
- The default/fallback visualization when a program has too few numeric variables
  for a vector field but you still want to *see it run*.

---

## 2. The object

Let the program define a transition relation `T ⊆ S × S` over state space
`S = D₁ × … × Dₖ` (one factor per state variable). Pick a seed `s₀ ∈ S` and
follow one successor chain

```
s₀ → s₁ → s₂ → … → s_N ,   s_{t+1} ∈ T(s_t)
```

of length `N` ticks. For each variable `vⱼ` with domain `Dⱼ`, the diagram draws
the **time series** `t ↦ (vⱼ value in s_t)`, `t ∈ {0..N}`, as a waveform inside a
horizontal band (lane) of unit height. The full object is the stack of `k` such
lanes sharing one time axis — a *waveform chart* in the sense of a digital-logic
timing diagram (Wakerly), generalized so numeric lanes carry analog traces.

Each lane is a function from ticks to a vertical position in `[base, base + 1]`:

- **Digital (bool/enum/string):** a step function, held constant between ticks
  and changing only on a vertical edge at a transition tick (zero-order hold).
- **Analog (int/real):** a piecewise-linear curve through the per-tick values,
  min–max normalized into the lane band.

---

## 3. Inputs

From the core machinery:

- `initial_state()` — candidate seed `s₀`.
- `successor(state)` — the lone next state (deterministic step).
- `successors(state, limit)` — the set-valued successor fan (used on
  nondeterministic discrete programs to avoid parking on a self-loop).
- `is_discrete()` — selects the walk strategy.
- `_key(state)` — canonical hash, for cycle/self-loop detection and the visited
  set.
- `state_vars` — the ranked, deduplicated variable list; **its order is the
  vertical stacking order** (rank #1 on top).
- `enum_variants[name]` — ordered variant list for enum lanes.
- `label(state)` — compact human label for the seed, shown in the title.

It consumes **no** phase-portrait machinery (no `assign_channels`, no
`facet_var`, no `reachable`): the timing diagram is a single trajectory across
all variables, so channel assignment collapses to "one lane per variable" and
the only ranking it needs is the vertical order.

---

## 4. Algorithm

### 4.1 Seed selection (`pick_seed`)

A flat trace (every value constant) is uninformative, so the seed is chosen to
*move*:

1. Let `init = initial_state()`. If `init` exists and
   `_key(successor(init)) ≠ _key(init)` — i.e. the initial state is **not** a
   fixed point — return `init`.
2. Otherwise (initial state is a fixed point, or absent) and the system has
   numeric variables: construct a **perturbed off-axis seed**. For each variable
   in rank order, assign:
   - int → `2800` for the first variable, `0` for the rest;
   - real → `2.8` for the first, `0.0` for the rest;
   - bool → `false`; enum → its first variant; string → `""`.
   This biases toward exciting a limit cycle whose fixed point sits at the origin
   (e.g. a Van der Pol oscillator seeded at rest). Use the seed only if it has a
   successor (`successor(seed) is not None`).
3. If no perturbation applies, fall back to `init` (the trace may be flat — a
   fixed point legitimately reads as a flat line).

### 4.2 Trajectory walk (`build_trace`)

Produce a list of up to `N+1` states (default `N = 40`):

1. `cur ← pick_seed`; `trace ← [cur]`; `visited ← {_key(cur)}`.
2. Choose a step rule: `prefer_change = is_discrete()`.
3. Repeat up to `N` times:
   - **Deterministic / numeric** (`prefer_change = false`): `nxt = successor(cur)`.
   - **Nondeterministic discrete** (`prefer_change = true`): query
     `succ = successors(cur, limit=32)`. Among these prefer a *state-changing*
     successor (`_key(s) ≠ _key(cur)`); among those prefer one *not yet visited*.
     Concretely: `changed = [s : _key(s) ≠ _key(cur)]`, `pool = changed or succ`,
     `fresh = [s in pool : _key(s) ∉ visited]`, pick `(fresh or pool)[0]`. This
     keeps the walk exploring instead of stalling on a self-loop, while still
     terminating into a cycle when no fresh state exists.
   - If `nxt is None`, stop early (chain died).
   - Append `nxt`, add `_key(nxt)` to `visited`, `cur ← nxt`.
4. **Pad to full width:** while `len(trace) < N+1`, repeat the last state. A chain
   that hit a fixed point or dead end thus holds its final value flat to the right
   edge, so every lane spans the whole time axis.

### 4.3 Layout

Let `k = len(state_vars)`, `n = len(trace)`, `ticks = 0..n-1`. Stack lanes
top-to-bottom in rank order so rank-#1 sits on top:

- Lane height `lane_h = 1`, inter-lane gap `gap = 0.55`.
- Variable at rank index `idx` (0 = top) gets baseline
  `base = (k − 1 − idx) · (lane_h + gap)`.
- Lane label: `#(idx+1)  name  [kind]`, placed at `base + lane_h/2`.
- Alternate lanes get a faint background band for legibility; a thin baseline
  grid line is drawn at each `base`.
- X-axis is the tick index; Y-axis carries the lane labels.

### 4.4 Per-lane encoding

For variable `v` of kind `K`, let `vals[t] = trace[t][v.name]`:

- **bool → digital step.** `y[t] = base + (lane_h if vals[t] else 0)`. Draw as a
  zero-order-hold step (`where="post"`) — value held until the next tick, then a
  vertical edge. Fill the area under the step faintly. Annotate lane rails `0` /
  `1`.
- **enum / string → ordinal lanes.** Establish a variant order:
  - enum: `enum_variants[name]` (declaration order);
  - string: the observed values sorted with `""` (sentinel/empty) first, i.e.
    key `(s != "", s)`.
  Map variant → ordinal index `0..nv-1`; place
  `y[t] = base + (order[vals[t]] / max(1, nv−1)) · lane_h`. Draw as a step
  (zero-order hold). **Print the variant name at each transition** (whenever
  `vals[t] ≠ vals[t−1]`) just above the held segment, so the lane reads as named
  states rather than anonymous levels. Label the bottom/top rails with the first
  and last variant.
- **int / real → analog.** Min–max normalize *within this lane*:
  `vmin = min(vals)`, `vmax = max(vals)`, `span = (vmax − vmin) or 1`,
  `y[t] = base + (vals[t] − vmin)/span · lane_h`. Draw a piecewise-linear line
  with small markers at each tick. Label the rails with `vmin` / `vmax` (the
  lane's actual data range — note each numeric lane is independently scaled).

### 4.5 Frame

X-limits `[−0.5, n−0.5]`; Y-limits span all lanes. X ticks every `max(1, n//20)`
steps with a faint vertical grid. A three-entry legend maps the three colors to
bool(digital) / enum-string(lanes) / int-real(analog). Title shows the FSM name,
the tick count `n−1`, and `label(seed)`.

---

## 5. Variable → channel mapping

The timing diagram does **not** compete for scarce channels — it allocates **one
track per variable**, so the only mapping decision is *track type* and *vertical
order*:

| Channel | Assignment | Reasoning |
|---|---|---|
| **x (shared)** | tick number `t` | Time is the universal independent axis; one shared axis aligns all tracks for vertical reading. |
| **y (within lane)** | the variable's own value | bool/enum/string → ordinal levels (categorical → position-as-level is faithful because there are few discrete levels); int/real → min–max-normalized position (quantitative → continuous position). |
| **vertical order** | importance rank from `state_vars` | Most-informative variable on top, so the eye reads the dominant signal first. |
| **color** | the *kind* of the track (digital/lane/analog), not a variable | Color is spent on encoding the track type, not on data; each lane's data already lives in its y-position. |

There is no facet and no size channel: stacking *is* the multivariate display,
which is exactly why this view scales to high dimension where a 2-D phase portrait
cannot.

---

## 6. Degradation & edge cases

- **No trajectory.** If `pick_seed` returns `None` (no initial state and no usable
  seed), or `build_trace` is empty, emit a placeholder frame: "N/A: no transition
  (no reachable trajectory)".
- **Fixed-point initial state, numeric system.** Handled by the off-axis
  perturbation seed (§4.1.2) so a limit cycle becomes visible instead of a flat
  line.
- **Fixed point with no usable perturbation.** Trace pads flat; every lane reads
  as a horizontal line at its seed value — a faithful picture of "this state is
  stable."
- **Nondeterministic discrete program.** The `prefer_change` walk (§4.2) avoids
  self-loop stalls and explores fresh states; once the reachable frontier from the
  seed is exhausted it settles into a cycle, padded flat thereafter.
- **Chain dies early** (state with no successor). Walk stops, last value is held
  to the right edge.
- **String variables with one observed value.** `nv = 1`; the ordinal denominator
  uses `max(1, nv−1)` so the lane flattens to its baseline rather than dividing by
  zero. Same guard for single-variant enums.
- **Constant numeric lane** (`vmax == vmin`). `span = (vmax − vmin) or 1`
  prevents division by zero; the lane draws flat at its baseline.
- **Mixed systems** are the native case: each lane independently selects digital
  vs analog from its own kind, so a single diagram carries enum + int + bool
  tracks side by side.

---

## 7. Parameters

| Parameter | Default | Meaning |
|---|---|---|
| `TICKS` (`N`) | 40 | Trajectory length (number of steps; `N+1` states). |
| `successors` limit (discrete walk) | 32 | Max successor fan queried per step when choosing a state-changing edge. |
| Perturbation seed (int) | 2800 / 0 | First numeric var / rest, for the off-axis seed. |
| Perturbation seed (real) | 2.8 / 0.0 | First numeric var / rest. |
| `lane_h` | 1.0 | Per-lane band height. |
| `gap` | 0.55 | Vertical gap between lanes. |
| X-tick stride | `max(1, n // 20)` | ~20 labeled ticks across the axis. |
| Per-lane numeric scaling | min–max | Each analog lane normalized to its own data range. |

`DIGITAL = {bool, enum, string}` defines the step-encoded kinds; everything else
is analog.

---

## 8. References

- **J. F. Wakerly, *Digital Design: Principles and Practices*** — timing/waveform
  diagrams and the zero-order-hold ("held flat, jumps on an edge") convention for
  digital signals; logic-analyzer display semantics.
- **Zero-order hold / sample-and-hold** (signal processing) — the step encoding
  of discrete-time signals between sample instants.
- **Small multiples / stacked horizon & sparkline tracks (E. Tufte,
  *The Visual Display of Quantitative Information*; *Beautiful Evidence*)** — one
  track per series sharing a common time axis, ordered by salience, for dense
  multivariate temporal reading.
- **Discrete dynamical systems / orbits** (e.g. Strogatz, *Nonlinear Dynamics and
  Chaos*) — a trajectory as the orbit `s₀ → s₁ → …` of the map; fixed points and
  limit cycles as the asymptotic behaviors the seed heuristic is built to reveal.
- **Ordinal encoding of categorical variables** (Bertin, *Sémiologie Graphique*;
  Munzner, *Visualization Analysis and Design*) — position-as-level for
  small-cardinality categoricals, with named-state annotation to recover the
  nominal meaning lost by the ordinalization.
