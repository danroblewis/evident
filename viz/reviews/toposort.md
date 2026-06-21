# toposort — visualization review

## What the program does

`toposort` is Kahn's algorithm run as an FSM over a fixed 5-node build DAG
(edges 0→2, 0→3, 1→3, 2→4, 3→4). The carried state is `Graph`: a per-node
remaining-in-degree vector (`d0..d4`), a per-node emitted flag (`o0..o4`), the
node `picked` this tick (-1 = none/done), and the output cursor `n_out`. Each
tick nondeterministically picks a READY node (degree 0, not yet emitted), latches
its output flag, decrements the in-degree of its successors, and advances `n_out`.
The reachable-state walk therefore enumerates EVERY valid topological order; the
sort completes at `n_out=5, picked=-1`, an absorbing fixed point. The genuinely
interesting state is the booleans + small integers (`d` in 0..2, `n_out` in 0..5,
`picked` in -1..4) — nothing here ever exceeds 5.

## Ranked best → worst

1. **morse_graph** — The clear winner: a clean DAG of the 13 reachable states laid
   out by `n_out` rows (0→5), branching where multiple nodes are ready and
   re-merging, terminating in a green-ringed absorbing state. This IS the lattice of
   topological orders — exactly what the program computes. Node labels show the real
   degree/output vectors. Faithful, readable, debuggable.
2. **state_graph** — Same 13-state/17-edge reachable graph, the diamond branch/merge
   structure visible and the terminal fixed point ringed; only loses to morse_graph
   because the node labels overprint each other into illegibility.
3. **reachability_tree** — BFS tree from the seed, 13 nodes / depth 6, showing the
   branching into distinct orders and the absorbing goal node (red ring). Honest and
   on-message; slight label overlap and the "unbounded — capped sample" caveat noted.
4. **time_series** — One concrete order traced tick-by-tick: `n_out` ramps 0→5 and
   flattens, `picked` rises then drops to -1 at completion, each `o` flag latches
   true in emit order, each `d` counter monotonically decrements to 0. A perfect
   single-trajectory sanity check of Kahn's mechanics.
5. **timing_diagram** — Same trajectory as a logic-analyzer trace over 40 ticks;
   the bool `o` lanes latching and the int counters settling reads well, and the long
   post-completion flat region correctly shows the absorbing halt. Redundant with
   time_series but a nice digital view of the latch ordering.
6. **parallel_coords** — All 13 reachable states as polylines across the 10 state
   axes, colored by `o2`. Genuinely shows the small bounded ranges (n_out 0–5,
   d's 0–2) and that `d0` is pinned at 0. Busy but truthful.
7. **fixedpoint_map** — Correctly finds exactly ONE fixed point (`n_out=5, picked=-1,
   all o=true`, starred) and scatters reachable states by n_out/picked. Right answer,
   though the two-facet split is awkward and most panel is empty.
8. **cobweb** — Treats `n_out` as a 1-D map: the staircase `x_{n+1}=x_n+1` climbing
   0→5 is an accurate picture of the output cursor incrementing, but it's a trivial
   counter and the two `o4` facets are identical. Mildly informative.
9. **chord_diagram** — Reduces everything to `o2` false→true with one arc. Technically
   correct (o2 latches once and never flips back) but throws away the entire algorithm
   to show a single boolean edge.
10. **scatter_matrix** — 10×10 grid of mostly-empty panels; the `±2500` axes on
    `n_out`/`picked` are FABRICATED (real range is -1..5), so most cells are blank or
    show two stray off-scale seed points. Noise.
11. **orbit_scatter** — Actively misleading: plots seeds at `n_out≈2700` and
    `picked≈1500` with axes spanning -1000..3000. These are sampled-from-thin-air
    numeric seeds the program can never occupy; the "attractor" framing invents
    dynamics that don't exist.
12. **basin_map** — Claims "1 basin" with a uniform grid of identical blue squares and
    a single attractor star at (6,-1). The grid is just the hardcoded sample lattice;
    it reads as if the whole plane flows to one attractor, which is vacuously true only
    because the off-trajectory cells are meaningless.
13. **nullcline_field** — The worst: a `±3500` vector field over `(n_out, picked)`
    with sign-region shading and arrows, on a program whose values never leave 0..5.
    Pure fabrication — there is no continuous flow here, and the giant axes invent a
    phase plane that has nothing to do with a discrete topo-sort.
14. **phase_portrait** — `±4000` axes faceted by `o4`, a near-empty grid of stray
    markers. The real `(n_out, picked)` cloud lives in a tiny corner; the hardcoded
    wide sampling buries the actual behavior.
15. **occupancy_heatmap** — Failed: renders "N/A — no visited states (transition
    unsat)". The walker couldn't seed/step this program, so it produced nothing.

**Missing:** `transition_matrix__toposort.png` did not render (present for 15 other
programs, absent here).

## Verdict

**Keepers:** `morse_graph`, `state_graph`, and `time_series`. The first two render the
actual object the program computes — the branching/merging lattice of valid topological
orders ending in one absorbing state — and time_series proves a single order's mechanics
tick-by-tick. reachability_tree is a fine fourth.

**Drop for this program:** every continuous/numeric renderer —
`nullcline_field`, `phase_portrait`, `basin_map`, `orbit_scatter`, `scatter_matrix`.
They impose a thousands-wide continuous phase plane on a discrete bounded counter and
fabricate vector fields, basins, and attractors that the program never exhibits.

**Notable finding:** the hardcoded ~±3000 Int sampling is clearly visible here.
`orbit_scatter` and `nullcline_field` plot states at `n_out≈2700` / `picked≈1500`
and arrows over `±3500`, when the program's entire reachable range is `n_out∈0..5`
and `picked∈-1..4`. Worse, `occupancy_heatmap` reports "transition unsat / no visited
states" — a real bug where the walker failed to seed this FSM at all — and
`transition_matrix` silently failed to produce a file. Three of the discrete-state
views (morse/state/reachability) prove the FSM is perfectly well-behaved, so the
heatmap's "unsat" is a generator defect, not a property of the program.
