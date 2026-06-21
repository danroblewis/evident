# Visualization review — `du`

## What the program does

`du` models the classic recursive disk-usage walk as an FSM over a fixed 5-node
tree. The carried state is a **traversal frontier**: five enum slots `s0..s4 ∈
{Unseen, Pending, Visited}` plus three Ints — `current` (the node popped this
tick, `-1` = done), `visited` (count of pops, climbs 0→5), and `total_size` (the
accumulating byte total, climbs 0→330). Each tick pops one `Pending` node, marks
it `Visited`, pushes a directory's children `Unseen→Pending`, and adds the node's
size. The dynamics are a **monotone, terminating BFS/DFS**: every status only ever
advances Unseen→Pending→Visited, the two counters only rise, and after ~5 ticks
the system parks forever at `(current=-1, all Visited, visited=5, total_size=330)`.
`current` is the only variable that ever decreases (it drops to `-1` at exhaustion).
There is exactly **one fixed point** and it is an absorbing terminal state, not a
cycle.

## Ranked best → worst for this program

1. **time_series** — The honest portrait: 8 stacked lanes show `total_size`
   ramping 0→330, `visited` 0→5, `current` spiking to 4 then dropping to -1, and
   each `sN` stepping Unseen→Pending→Visited then flatlining. This IS the run.
2. **timing_diagram** — Same story over 40 ticks, making the "everything freezes
   after tick 5" termination unmistakable; the enum lanes are labeled inline. A
   touch redundant with time_series but the long flat tail is the clearest proof
   of halting.
3. **morse_graph** — Correctly condenses the reachable set into a clean top-to-
   bottom DAG keyed by `total_size` (0→65→100→130→200→330), capturing the
   traversal-order branching and the single terminal sink. Faithful and compact.
4. **reachability_tree** — 18-node BFS tree from the seed, branching on pop order
   and converging on the `(-1, …, 330, 5)` absorbing leaf (ringed). Shows the
   real state space; labels overlap badly but the shape is right.
5. **parallel_coords** — 18 reachable states across all 8 axes; you can read the
   monotone ladders (total_size 0→330, every sN ending Visited, s0 always
   Pending→Visited). Cluttered but genuinely about this program's states.
6. **transition_matrix** — Sparse near-diagonal heatmap = a near-deterministic
   march with a little pop-order fan-out. Accurate but the state labels are an
   unreadable wall.
7. **state_graph** — Real reachable graph (18 states, 25 edges) with the terminal
   ringed, but the node labels are stacked into illegible mush; you trust it more
   than you can read it.
8. **chord_diagram** — Reduces to one `sN`'s Unseen→Pending→Visited flow. Truthful
   but trivial — it's just one arrow chain, which 7 other plots already show.
9. **fixedpoint_map** — Correctly finds **1** fixed point (the red star at
   `total_size≈330, current=-1`), which is the right answer, but two of three
   facets are nearly empty and the scatter is otherwise just reachable states.
10. **scatter_matrix** — 210 "sampled" states; the off-diagonal panels are sparse
    dotted grids that imply relationships between Ints that span thousands. Mostly
    noise for a program whose real values are tiny.
11. **cobweb** — A perfect `y=x` diagonal across ±3000 with a red dot at 2000.
    `total_size` is a pure accumulator, not a 1-D map iterating toward a fixed
    point, so the cobweb framing is meaningless here — and 2000 is a value the
    program never reaches.
12. **phase_portrait** — Vector field sampled over total_size,current ∈ ±4000 with
    a "fixed point" star near 2000. The real trajectory lives in a 0–330 × (-1..4)
    sliver; this invents a flow over a region the FSM never visits.
13. **nullcline_field** — Sign-region arrows over ±3500 on both axes, splitting at
    `current≈0`. Pure artifact of sampling unreachable space; `du` has no
    continuous nullcline structure.
14. **basin_map** — **The worst offender.** It fabricates **8 distinct attractor
    basins** with attractors at total_size ≈ 3201, 1520, 65, 2881, 829, -594,
    2251, -2763 — all but one of which the program can never produce (negative
    sizes, sizes in the thousands). `du` has exactly ONE attractor at 330; this
    plot invents seven phantom ones by iterating from seed points in the
    hardcoded ±3000 range.

## Verdict

**Keepers:** `time_series`, `timing_diagram`, and `morse_graph` — between them you
get the true trajectory, the proof of termination, and the condensed reachable
structure. Add `reachability_tree` if you want the branching state space.

**Drop for this program:** every numeric-field renderer — `basin_map`,
`nullcline_field`, `phase_portrait`, `cobweb`, `scatter_matrix`. They all sample
Int axes over a hardcoded ~±3000 window the program never enters (real ranges:
total_size 0–330, current -1–4, visited 0–5), so they paint flow, basins, and
nullclines over empty space.

**Notable finding:** `basin_map` is a textbook case of the known fabrication bug —
it reports **8 attractor basins with attractors at values like -2763, 3201, and
1520** for a program that monotonically accumulates to a **single** value of 330
and halts. The numeric sampler manufactures an entire multi-basin dynamical system
out of a deterministic accumulator. `cobweb` corroborates: it draws a `y=x`
diagonal and plants the "fixed point" at 2000, a value `total_size` cannot reach.
The trustworthy fixed-point answer comes only from `fixedpoint_map`, which scans
the *reachable* set and correctly finds exactly one (at 330, current=-1).

**Missing:** `occupancy_heatmap` and `orbit_scatter` did not render (no PNG) — both
are numeric renderers that would almost certainly have suffered the same ±3000
fabrication, so their absence costs nothing here.
