# Visualization review — `histogram`

## What the program does

`histogram` is a binning accumulator. A cursor walks a fixed 10-element
numeric stream `⟨10, 30, 60, 90, 25, 49, 74, 75, 0, 50⟩` one sample per tick,
classifies each value into one of four half-open bins (`bin0 v<25`, `bin1
25≤v<50`, `bin2 50≤v<75`, `bin3 v≥75`), and bumps the matching counter. The
carried STATE is a single `Hist` record: `cursor`, `bin0..bin3`, and `total`.
The transition is purely monotone: each non-done tick does `cursor += 1`,
`total += 1`, and `+1` to exactly one bin (so `total = bin0+bin1+bin2+bin3`
always holds). Once `cursor ≥ 10` the histogram freezes — that final state
`(cursor=10, bin0=3, bin1=3, bin2=3, bin3=2, total=10)` is the lone fixed point.
There are no cycles, no branches, no negative values, and the entire reachable
set is a single 11-node chain. The dynamics are about as simple as a daemon
gets, which makes this an excellent stress test for whether a renderer reports
the REAL reachable set or fabricates structure off a grid sweep.

## Ranked best → worst

1. **time_series** — Five stacked panels (`total`, `bin0..bin3`) over ticks
   showing every counter's exact staircase: `total` climbs 0→10 linearly, each
   bin steps up only when a matching sample arrives, all flatline at the freeze.
   This IS the program; nothing is invented.
2. **morse_graph** — The clean 11-node condensation chain `total=0 … total=10`,
   each node carrying the full `[bin2 bin1 bin3 bin0]` breakdown, source node
   red, terminal green. Exact, readable, and shows the fold accumulating.
3. **fixedpoint_map** — The model citizen of the numeric renderers: scans the
   *reachable* set (11 states), honest axes (`total` 0–10, `bin2` 0–3), faint
   dots tracing the real scan path, single true fixed point `(10, 3)` starred.
4. **reachability_tree** — 9-node BFS chain (depth 8), full state tuple per node,
   shade deepening with depth. Faithful to the deterministic linear scan; loses
   a point only for overlapping tuple labels.
5. **state_graph** — Correct 11-state / 11-edge chain ending in a self-looping
   terminal, but the node labels overprint each other into illegible mush.
6. **timing_diagram** — Same staircase content as time_series over 40 ticks
   (30 of them flat post-freeze); faithful but redundant, and its
   digital/lanes legend is dead weight (no bool/enum state exists).
7. **cobweb** — The `total → total+1` staircase against `y=x` reads correctly,
   but it extends the line into negative `total` the program never reaches.
8. **scatter_matrix** — The integer bin×bin panels genuinely show the joint bin
   distribution, but the `total` and `bin2` axes are blown out to ±3000 from
   grid sampling, polluting half the grid with phantom states.
9. **chord_diagram** — Bins `total` into `[-12,12]`; the +2-step path happens to
   read as a clean walk, but the `-10 → -8 → -4 → -2` negative-total nodes are
   pure fabrication — `total` is never negative.
10. **basin_map** — Reports "1 basin" (true — everything converges to the final
    histogram) but its attractor label says `total≈12` (actual final total is
    10) and it samples a 90-seed grid the program never enters; one flat color.
11. **parallel_coords** — Broken: `total` axis ±1500, every bin axis collapsed to
    `-1/0/1`, trajectories fan out from fabricated `total` values to a degenerate
    zero. Conveys nothing true.
12. **basin / orbit_scatter** — 4 random seeds scattered across ±3000, each
    frozen at tick 0–2 with no visible orbit. Invents an "attractor" map for a
    program that has one real trajectory.
13. **occupancy_heatmap** — ±2500 axes with bright cells scattered at random-seed
    freeze points; the real attractor `(10, 3)` is a single near-origin pixel not
    even visible. Actively misleading about "where the system dwells."
14. **phase_portrait** — The textbook fabrication artifact: a ±4000 plane carpeted
    edge-to-edge with red "fixed point" stars and a few stray arrows. A program
    whose values live in `[0,10]` is rendered as a uniform field of invented
    equilibria. Worst offender.

(`transition_matrix` did not render — no PNG was produced for this program.)

## Verdict

**Keepers (2–3):** `time_series` is the single most informative+representative
diagram — it shows every carried counter's exact evolution and the freeze, with
zero fabrication. `morse_graph` is the best topological view (the 11-node chain
with per-node bin breakdown). `fixedpoint_map` is the one numeric renderer that
gets it right by scanning the reachable set instead of a grid.

**Drop:** `phase_portrait`, `occupancy_heatmap`, `orbit_scatter`,
`nullcline_field`, `basin_map`, and `parallel_coords` are all worthless for this
program — each grid-samples a hardcoded ±3000-ish range the program never enters
and manufactures fixed points / basins / sign-region fields out of nothing.
`chord_diagram` is borderline (clean path but negative-total nodes are fake).
`timing_diagram` is redundant with `time_series`.

**Notable finding:** This program cleanly separates the renderers into two
families. Those that **scan the reachable set** (time_series, morse_graph,
fixedpoint_map, reachability_tree, state_graph, cobweb) are all faithful. Those
that **grid-sample a hardcoded numeric range** (phase_portrait, basin_map,
occupancy_heatmap, orbit_scatter, nullcline_field, and the `total`/`bin2` axes of
scatter_matrix & parallel_coords) all fabricate structure — phantom equilibria,
phantom basins, phantom nullcline diamonds, and negative `total` values — for a
program whose entire state space is an 11-step monotone chain in `[0,10]×[0,3]`.
The smoking gun is `basin_map` reporting `total≈12` and phase_portrait/occupancy
placing the "attractor" at ±2500 when the real fixed point is `(10, 3)`. This
strongly corroborates the known ±3000 hardcoded-sampling bug.
