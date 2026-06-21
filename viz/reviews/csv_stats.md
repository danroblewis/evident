# csv_stats — visualization review

## What the program does

`csv_stats` is a streaming numeric aggregator. Its carried state is a `Stats`
record of five Ints — `cursor`, `count`, `sum`, `min`, `max`. A cursor walks a
fixed 6-cell column `[40, 15, 80, 23, 80, 4]` one cell per tick; each value folds
into the running `count`/`sum`/`min`/`max`. `min` is seeded to a high sentinel
(`1000000`), `max` to a low one (`-1000000`), so the first value wins both. Once
`cursor ≥ 6` the aggregates freeze (fixpoint → halt). The real trajectory is a
single 7-state line: `(0,0,0,1e6,-1e6) → … → (6,6,242,4,80)`. It is monotone,
absorbing, and has exactly one reachable orbit — there are NO cycles, NO basins,
and NO 2-D attractor structure. The actual numeric ranges are tiny: sum tops out
at 242, min/max in [4, 80] after the first tick.

## Ranked best → worst

1. **time_series** — Three stacked panels show exactly the real run: sum climbing
   0→242, min collapsing from the 1e6 sentinel to a floor, max jumping off the
   -1e6 sentinel and pinning. Honest, immediately readable, the sentinel→data
   transition is the whole story and it shows it.
2. **morse_graph** — Exact reachable scan: 7 boxes, each labeled with the true
   `sum`/`min`/`max`, sentinel-seeded root in red, frozen `sum=242 [min=4 max=80]`
   terminal in green. This is the program's behavior with zero fabrication.
3. **reachability_tree** — Same 7-node linear chain, root and absorbing node ringed,
   each node carrying the real tuple. A faithful, if visually sparse, twin of the
   morse graph; the long header honestly flags "reachable set unbounded — capped".
4. **timing_diagram** — 40-tick analog traces of sum/min/max with correct y-labels
   (242, 4, 80, and the 1e6/-1e6 sentinels). Shows the freeze-after-tick-5 plateau
   clearly; slightly redundant with time_series but adds the "stays frozen" proof.
5. **fixedpoint_map** — Correctly reports 1 fixed point at `sum=242, min≈0` from a
   reachable scan; only flaw is the lone 1e6 sentinel state stretching the y-axis
   so the 6 real states bunch along the bottom.
6. **parallel_coords** — Sum axis uses a plausible ±1500 range, but the min/max axes
   collapse to a single point at 0, so the only visible structure is a fan on `sum`.
   Half-honest: shows nothing about the aggregation, mildly misleading axes.
7. **chord_diagram** — Bins `state.sum` over a fabricated `[-290, 290]` range that
   the program never enters (real sum is 0→242, never negative), draws meaningless
   self-loops on each bin. Pretty, says nothing true.
8. **orbit_scatter** — Plots 4 random seeds (sum ≈ -1500..2800, min 0..2700) that
   are NOT reachable states; every "orbit" is one dot because each seed is already
   frozen. Invents a seed cloud the program would never produce.
9. **scatter_matrix** — 210 sampled states scattered across `sum ∈ [-3000, 3000]`,
   `min`/`max` at 0 or 1e6. The off-diagonal panels are just two horizontal lines —
   sampling artifact, no real correlation, fabricated range.
10. **state_graph** — Two overlapping blobs in the top-right corner with completely
    illegible smashed-together labels (`(0,...)` and `(6,...)` collide). Technically
    only 2 nodes plotted, but unreadable.
11. **cobweb** — The classic fabricated-structure case: holds min/max at sentinels
    (making `done` true and the map the identity), so `f(sum)=sum` plots as a perfect
    diagonal from -3000 to 3000. Suggests a continuum of fixed points; the program
    has one. Pure artifact of the ±3000 sampling.
12. **basin_map** — A dense unreadable grid of tiny colored dots with a giant
    illegible legend; no basins exist (single absorbing state), so it fabricates a
    field over a range the program never visits.
13. **nullcline_field** — Worst. A uniform black dot-grid over `sum,min ∈ ±3500`
    with overlapping legend text; claims "sign-regions of (dsum, dmin)" for a
    program that is a finite forward scan with no vector field at all. Completely
    fabricated, zero signal.

(`transition_matrix` did not render — no `transition_matrix__csv_stats.png` exists;
`occupancy_heatmap` rendered an explicit "N/A — no visited states (transition unsat)"
empty card, which at least failed honestly.)

## Verdict

**Keepers:** `time_series`, `morse_graph`, `reachability_tree` (with `timing_diagram`
as a near-tie 4th). These three tell the true story — a monotone 7-state scan that
folds 6 cells and freezes — with no invented structure.

**Drop for this program:** every numeric-vector-field renderer —
`nullcline_field`, `cobweb`, `basin_map`, `phase_portrait`, `orbit_scatter`,
`scatter_matrix`, `chord_diagram`. They all sample Int axes over a hardcoded
±3000/±3500 (or chord's ±290) range the program never enters and consequently
fabricate continua, seed clouds, and sign-regions that do not exist. `state_graph`
is correct-but-illegible (label collision).

**Notable finding:** the `phase_portrait` is the textbook fabricated-structure
artifact — a full 18×18 grid of red "fixed point" stars spanning ±1.3e6 on both
`state.sum` and `state.min`. The real program has exactly ONE reachable fixed
point (`sum=242`) and never leaves `sum ∈ [0,242]`. The renderer declares the
entire sampled plane stationary because it samples states the FSM can't reach
(where `done` is already true, so nothing moves), inventing ~300 fixed points
where there is one. `cobweb`'s identity diagonal and `nullcline_field`'s uniform
grid are the same bug in different clothing. The exact-reachable renderers
(`morse_graph`, `reachability_tree`, `fixedpoint_map`, `time_series`,
`timing_diagram`) all get it right precisely because they trace the actual orbit
instead of sampling a fabricated box.
