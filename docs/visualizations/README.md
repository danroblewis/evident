# Evident visualization methods — algorithm specs

Language- and framework-agnostic specifications of how Evident programs are
visualized, written so a future implementation (a web IDE, a different language)
can rebuild each method from the math and algorithms alone — independent of the
reference Python/Z3/matplotlib code in `viz/`.

**Start here:** [`00-core-machinery.md`](./00-core-machinery.md) — the shared layer
every method depends on: the transition-relation IR, the solver-as-dynamics-oracle
queries, variable ranking/dedup (mRMR), the channel-mapping fitness table, the
facet guard, and the interestingness sort. Every method spec below references it.

Each method doc follows the same shape: *what it shows · the mathematical object ·
inputs (which transition queries) · the algorithm · variable→channel mapping ·
degradation across program types · parameters · references.*

## Methods by family

**Dynamics in state space** (sample the flow; numeric-leaning)
- [`phase_portrait.md`](./phase_portrait.md) — vector field of `f(x)−x` + trajectories
- [`orbit_scatter.md`](./orbit_scatter.md) — discrete-time orbit as time-colored dots
- [`cobweb.md`](./cobweb.md) — 1-D map `x_{n+1}` vs `x_n` against the diagonal
- [`nullcline_field.md`](./nullcline_field.md) — sign-regions of each component's change
- [`fixedpoint_map.md`](./fixedpoint_map.md) — fixed points / cycles in a 2-axis projection
- [`basin_map.md`](./basin_map.md) — color a state-space slice by which attractor it reaches

**Discrete structure** (enumerate the transition relation; finite-state)
- [`state_graph.md`](./state_graph.md) — the reachable state-transition graph
- [`morse_graph.md`](./morse_graph.md) — SCC condensation = the recurrence skeleton
- [`reachability_tree.md`](./reachability_tree.md) — BFS unrolling from the initial state
- [`transition_matrix.md`](./transition_matrix.md) — adjacency matrix heatmap
- [`chord_diagram.md`](./chord_diagram.md) — transition flow between values of one variable

**Time domain** (axis = tick)
- [`time_series.md`](./time_series.md) — each variable vs tick
- [`timing_diagram.md`](./timing_diagram.md) — EE-style waveform tracks per variable

**Density & high-dimensional**
- [`occupancy_heatmap.md`](./occupancy_heatmap.md) — 2-D histogram of where the system spends time
- [`scatter_matrix.md`](./scatter_matrix.md) — pairwise projections of the sampled states
- [`parallel_coords.md`](./parallel_coords.md) — all variables as parallel axes, lines colored by class

## Design background (the "why")

- [`../design/visualization-survey.md`](../design/visualization-survey.md) — the
  landscape (program/prover visualization) and where this lens sits.
- [`../design/portrait-axes.md`](../design/portrait-axes.md) — why axes default to
  the interface; witness/indicator variables.
- [`../design/phase-portraits-research.md`](../design/phase-portraits-research.md) —
  the standard phase-portrait treatment (fixed-point classification, the
  conservative-vs-dissipative honesty rule).
- [`../design/morse-graphs.md`](../design/morse-graphs.md) — the rigorous
  global-dynamics pipeline (Conley–Morse) behind the set-valued / reachable view.
