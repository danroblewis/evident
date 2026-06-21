# Transition matrix

A heatmap of the transition relation as an **adjacency matrix**: a representative
set of states is ordered along a single shared axis (used for both rows and
columns), and cell `(i, j)` is lit iff there is a one-step transition from state
`i` to state `j`. It answers, *globally*: "which states flow to which?" — the
gross topology of the difference equation, before any projection onto two named
variables.

Read this together with [`00-core-machinery.md`](00-core-machinery.md), which
defines the shared transition queries (`successor`, `successors`, `reachable`,
`initial_state`), the variable ranking / classification (`state_vars`,
`numeric_vars`, `categorical_vars`, `var_class`, `enum_variants`), and the
labeling / state-key helpers (`label`, `_key`). This doc specifies only the
matrix-specific algorithm.

## 1. What it shows

The transition relation `state = f(_state)` is, abstractly, a directed graph (or
multigraph, since `f` may be nondeterministic) on the state space. The matrix is
the **adjacency matrix of that graph** rendered as an image. Structure visible in
the image:

- **Diagonal cells** — fixed points (a state mapping to itself).
- **Off-diagonal bands** — limit cycles / drift (state `i` flows to a nearby
  ordered state `i±k`, giving a band parallel to the diagonal).
- **Block structure** — when states are ordered so a categorical variable forms
  contiguous blocks, a block-diagonal pattern means "transitions stay within a
  mode"; off-block cells mean "mode switches".

When to use it:

- **Discrete programs** (only bool / enum / string state): the matrix is *exact*
  and complete — every reachable state and every edge, nothing sampled. This is
  the canonical, lossless view of a finite-state machine.
- **Numeric / mixed programs**: the state space is infinite, so the matrix shows
  a **coarse-grained flow** over a sampled grid. Useful as an overview of global
  structure (basins, cycles) that a 2-variable phase portrait projects away, at
  the cost of binning resolution.

## 2. The object

Given an ordered finite state list `S = [s_0, …, s_{n-1}]`, the object is the
`n × n` matrix `A` with

```
A[i][j] = 1   if  s_j ∈ successors(s_i)   (binned to nearest sampled state)
          0   otherwise
```

rendered as a heatmap (row `i` = "from" axis, column `j` = "to" axis), with the
*same* ordering on both axes. A secondary categorical channel is drawn as two
**color ribbons** (one beside the rows, one above the columns) plus tinted tick
labels, encoding the value of the top categorical variable per ordered state.

## 3. Inputs

From core machinery:

- `is_discrete()` — selects the exact vs. sampled branch.
- `reachable()` → `(states, edges)` — exact graph for discrete programs; `edges`
  is a set of index pairs into `states`.
- `successors(state, limit)` — set-valued one-step image, for sampled programs.
- `successor(state)` — single step, used during range inference.
- `initial_state()` — a seed for range inference.
- `state_vars`, `numeric_vars`, `categorical_vars`, `var_class`, `enum_variants`
  — ordering and the ribbon channel.
- `label(state)`, `_key(state)` — axis tick labels and permutation bookkeeping.

## 4. Algorithm

### A. Build the state set

**Discrete branch** (`is_discrete()` true): call `reachable()` to get the exact
`states` and `edges`. No sampling.

**Numeric / mixed branch**: build a representative grid (`sample_states`):

1. For each variable in `state_vars`, build a list of axis sample values by kind:
   - `bool` → `[false, true]`.
   - `enum` → its full `enum_variants` list.
   - `int` / `real` → `num_grid` points of `linspace(lo, hi, num_grid)` over an
     inferred range (step B); ints rounded and deduplicated.
   - `string` / other → a single placeholder `[""]`.
2. Take the **Cartesian product** of all axes → candidate states.
3. **Cap** the product at 64 states by selecting `linspace`-spaced indices
   (an even subsample preserving spread), to keep the matrix legible and cheap.

`num_grid` defaults to `9` when the state is purely numeric (the matrix is the
*only* place flow shows, so spend resolution there) and `5` when any bool/enum
axis is present (those already multiply the count up).

### B. Numeric range inference (`infer_numeric_range`)

We cannot trust the initial state alone for a sampling window — it may be a fixed
point (e.g. an origin equilibrium) from which probing never moves. So:

1. Build a base state with every variable zeroed/defaulted (bool→`false`,
   enum→first variant, string→`""`, numeric→`0`).
2. **Seed set** = the initial state, plus, for each numeric axis, four off-axis
   probes at multiples `{-1, -0.4, 0.4, 1}` of a wide default `span = 3200`
   placed on that axis (base state otherwise). This casts a coarse net so we
   discover the operating magnitude even when the origin is an equilibrium.
3. From each seed, **follow the orbit forward** up to 60 steps via `successor`,
   recording every value the target variable visits.
4. Let `mag = max(|min|, |max|)` of visited values. If `mag > 1`, return the
   symmetric window `[-1.15·mag, +1.15·mag]` (15% margin). Otherwise fall back to
   `[-span, span]`.

### C. Build the matrix (`build_matrix`)

- **Exact (discrete)**: initialize `A = 0`; for each `(i, j) ∈ edges`, set
  `A[i][j] = 1`. (Edges already index into the ordered state list — see step D.)
- **Sampled**: for each state `s_i`, query `successors(s_i, limit=16)`; for each
  successor `t`, find the nearest sampled state index `j` (step C′) and set
  `A[i][j] = 1`.

**C′. Nearest-state binning** (`nearest_index`): the sampled successor `t`
generally is not itself a grid point, so map it to the closest sampled state:

- **Discrete axes must match exactly** — any mismatch on a bool/enum/string axis
  disqualifies that candidate entirely.
- Among candidates matching on all discrete axes, pick the one minimizing the
  **squared Euclidean distance** over numeric axes:
  `d = Σ_{numeric v} (t.v − s.v)²`. Ties broken by first index.

This is a product metric: exact equality on the categorical sub-space, Euclidean
on the numeric sub-space.

### D. Order the states (`order_states`)

The matrix axis is shared, so the only design choice is the **state ordering** —
chosen to make structure legible. Returns `(ordered_states, ribbon_var,
ribbon_values)`.

- **If a categorical variable exists**: pick `ribbon_var = categorical_vars[0]`
  (the top-ranked categorical). Sort states by a key:
  - **Primary**: that categorical's value, ranked by *declared variant order* for
    enums, `int(false) < int(true)` for bools, lexicographic for strings — so its
    values form contiguous blocks.
  - **Secondary** (stable within block): remaining categoricals (as strings),
    then numerics ascending.
  `ribbon_values[i]` = the ribbon var's value at ordered position `i`.
- **Purely numeric** (no categorical): order by the tuple of all numeric values
  (primary numeric axis first). The limit-cycle flow then reads as an
  off-diagonal band. `ribbon_var` = the primary numeric axis; ribbon encodes it
  as a magnitude gradient.
- **No variables at all**: identity order, no ribbon.

**Discrete edge remap**: for the exact branch, ordering permutes the state list,
so the exact edge index-pairs must be carried through the permutation. Build
`pos[_key(s)] = original_index`, then `perm[i] = pos[_key(ordered[i])]` and its
inverse `inv[old] = new`; rewrite each `(i, j) ∈ edges` to `(inv[i], inv[j])`.
(Sampled branch needs no remap — its matrix is built *after* ordering.)

### E. Render

- Draw `A` as an `n × n` heatmap, `vmin=0, vmax=1`, equal aspect, nearest-neighbor
  interpolation (no smoothing — cells are discrete). A sequential colormap
  ("no"=dark → "yes"=bright) with a 2-tick colorbar labeled no/yes.
- Row `i` (y-axis) = "from state", column `j` (x-axis) = "to state"; both axes
  ticked with `label(s_i)` (monospace). A faint minor grid separates cells.
- If a ribbon var exists, draw the two color ribbons (step 5) and tint the tick
  labels to match.

## 5. Variable → channel mapping

This viz is deliberately *not* a 2-variable projection. The full state vector
occupies the strongest perceptual channel — **position** along the shared
matrix axis (Cleveland–McGill ranking) — via the ordering, not via two chosen
variables. The matrix cells themselves are a neutral transition heatmap; their
"value" is binary edge-existence.

The single discretionary channel is **hue**, used honestly for one categorical
attribute (categorical → color is the type-appropriate mapping):

| Channel | Carries | Encoding |
|---|---|---|
| Position (row/col order) | whole state vector | ordering by top categorical, then numerics |
| Color (side ribbons + tick tint) | `categorical_vars[0]` value | qualitative palette (≤10 → 10-color; else 20-color), stable per distinct value |
| Color (numeric fallback) | primary numeric axis value | sequential magnitude gradient, normalized `(v−lo)/(hi−lo)` |
| Cell intensity | edge existence | binary 0/1 |

The ribbon makes same-mode blocks readable at a glance: a block-diagonal-ish
matrix relative to the ribbon blocks = "transitions stay within a mode".

## 6. Degradation & edge cases

- **No states obtainable at all** (e.g. `initial_state` / `reachable` fail, or an
  exception while building) → emit a **placeholder** image titled with the FSM
  name and the reason / state kinds, rather than a blank or a crash.
- **Pure numeric, no categorical** → no ribbon blocks; states ordered by numeric
  value, ribbon degrades to a magnitude gradient legend (colorbar).
- **String axes** → not griddable; collapsed to a single `""` placeholder value
  (so they don't explode the Cartesian product but also contribute nothing).
- **Sampled-state aliasing** → because successors are binned to nearest, distinct
  successors can collapse onto the same column; the matrix shows *coarse-grained*
  flow, not exact dynamics. Mitigated by spending resolution (`num_grid=9`) when
  the state is purely numeric.
- **Large state sets** → capped (discrete: bounded by `reachable`'s own limit;
  sampled: 64 states) to keep the `O(n²)` image and the per-state successor
  queries tractable. Figure side and font size scale with `n`.

## 7. Parameters

| Parameter | Default | Meaning |
|---|---|---|
| `num_grid` | 9 (pure numeric) / 5 (has discrete axis) | grid points per numeric axis |
| state cap (sampled) | 64 | max sampled states after Cartesian product, via even subsample |
| `successors` limit | 16 | max next-states queried per sampled state |
| orbit follow depth | 60 | steps to follow each range-inference seed |
| `span` | 3200 | default numeric window half-width / probe magnitude |
| probe multipliers | {−1, −0.4, 0.4, 1} | off-axis seed positions per numeric axis |
| range margin | 1.15× | symmetric padding around discovered magnitude |
| `reachable` limit | 5000 (core) | max exact states for the discrete branch |

## 8. References

- **Adjacency-matrix visualization of graphs**, as an alternative to node-link
  layouts for dense or block-structured graphs — Ghoniem, Fekete & Castagliola,
  *On the Readability of Graphs Using Node-Link and Matrix-Based
  Representations* (InfoVis 2004); Henry & Fekete, *MatrixExplorer* (2006). The
  ordering-makes-structure insight (block-diagonal under a good permutation) is
  the **matrix reordering / seriation** literature — Liiv, *Seriation and matrix
  reordering methods* (2010); Behrisch et al., *Matrix Reordering Methods*
  (EuroVis STAR, 2016).
- **Channel-effectiveness ranking** (position > length > … > color for
  quantitative; color/shape for categorical) — Cleveland & McGill (1984);
  Mackinlay, *Automating the Design of Graphical Presentations* (1986).
- **Difference equations as directed state graphs**, fixed points and limit
  cycles as diagonal / off-diagonal structure — Strogatz, *Nonlinear Dynamics and
  Chaos*; standard discrete-dynamical-systems framing.
- **Grid sampling + nearest-bin successor approximation** of a continuous map onto
  a finite transition graph is the **set-oriented / Ulam–Galerkin / GAIO**
  approach to approximating transfer operators and almost-invariant sets —
  Dellnitz & Junge, *On the Approximation of Complicated Dynamical Behavior*
  (1999).
