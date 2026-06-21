# randomwalk — visualization review

## What the program does

`randomwalk` is a Markov random walk on a fixed undirected 5-node graph
(A–E, adjacency A↔{B,C}, B↔{A,C,D}, C↔{A,B,E}, D↔{B,E}, E↔{C,D}). The carried
state is `node ∈ Node` (the walker's current vertex) plus five `Int` visit
counts `v0..v4`. Each non-seed tick the walker moves to a **nondeterministically
chosen neighbour** of the previous node — every neighbour is a valid successor, so
the transition is deliberately under-constrained — and increments that node's
count. The seed pins `node = A, v0 = 1, rest = 0`. The interesting object is the
**branching fan of reachable states**, not any attractor: the visit counts only
ever climb from 0 (small, non-negative, bounded by tick count) and there are no
fixed points or cycles in state-space. Four viz types failed to render
(basin_map, morse_graph, occupancy_heatmap, orbit_scatter — all numeric).

## Ranked best → worst

1. **reachability_tree** — the only diagram that shows the actual structure: BFS
   from the seed, A fans to {B,C}, each expands to its neighbours, nodes colored
   by `state.node`. This *is* the stochastic fan the program is about.
2. **chord_diagram** — a clean node→node flow graph; arc widths show which
   transitions the sampler took most (D↔E↔B↔C heavily). Slightly
   sample-biased (under-shows A's edges) but genuinely representative of the graph.
3. **time_series** — honest per-run trace: node lane is a clean A-B-A-B square
   wave, v0 and v1 climb linearly, v2/v3/v4 flat at 0. Truthful, and it exposes
   that Z3's chosen trace is degenerate (always picks the same successor).
4. **timing_diagram** — same content as time_series with a cleaner enum lane;
   reads the A↔B oscillation and the two climbing counts at a glance. Redundant
   with #3 but slightly more legible.
5. **transition_matrix** — diagonal-banded from→to heatmap over sampled states;
   structurally correct (sparse, near-diagonal as counts increment) but hard to
   read and the labels are unreadable at this size.
6. **parallel_coords** — dense ribbon over node + v0..v4; you can see counts hug 0
   and node spreads across A–E, but it's a tangle and adds little over #1.
7. **scatter_matrix** — the node/count panels are fine, but the v3/v4 panels
   inherit the fabricated −2500..2500 axis range, padding the plot with empty space
   the program never visits.
8. **fixedpoint_map** — title correctly says "no fixed points / short cycles
   found" (right answer for a random walk), but then draws a meaningless yellow
   v3=v4 diagonal to 1200; only the title text is informative.
9. **phase_portrait** — FABRICATED. Samples v3/v4 over −4000..4000 and stamps a
   red "fixed point" star on every grid cell. The real counts are small, monotone,
   non-negative; this invents a sea of attractors that do not exist.
10. **cobweb** — degenerate and fabricated-range. Picks v3 (held at 0 in the real
    trace), holds the other vars so f(x)=x, samples −3000..3000, and yields a
    perfect y=x diagonal calling every point a fixed point. Pure artifact.
11. **nullcline_field** — worst. A uniform pink field of black dots over
    −3000..3000 on (v3,v4), two counts that never move. No signal whatsoever.

## Verdict

**Keepers:** `reachability_tree` (the fan — the whole point of this program),
`chord_diagram` (the graph), and `time_series`/`timing_diagram` (one honest trace,
keep one of the two).

**Drop:** `phase_portrait`, `cobweb`, `nullcline_field`, and `fixedpoint_map`'s
plot. All four are numeric renderers that sample `v3`/`v4` over a hardcoded
±3000–4000 window the walk never enters, fabricating fixed points (red-star sea,
y=x diagonals, dot field) that contradict the program's real behaviour.
`scatter_matrix` is borderline-droppable for the same axis-range reason.

**Notable finding:** two things stand out. (1) The corroborated ±3000 hardcoded-
range bug fabricates structure on this program harder than usual — `nullcline_field`,
`phase_portrait`, and `cobweb` are *all pure noise* because v3/v4 are
monotone-from-zero counts, so the fabricated negative/huge regions are 100% of the
plotted area. (2) The honest dynamic traces (time_series, timing_diagram) reveal
the trampoline picks a **degenerate A↔B oscillation** every run — Z3 resolves the
nondeterministic neighbour disjunction the same way each tick, so a single trace
never demonstrates the under-constrained fan. Only the reachability_tree, which
enumerates *all* successors rather than following one solver choice, actually
exhibits the nondeterminism the program was written to show.
