# pstree — visualization review

## What the program does

`pstree` recovers a process forest from flat `(pid, ppid)` rows by computing each
process's tree DEPTH. The carried state is a `Forest` record: six per-process depth
slots `d0..d5` (−1 = not yet computed) plus a `cursor`. Each tick advances the cursor
to the next pid (clamped at 5) and latches that process's depth = parent's
already-computed depth + 1, exploiting the fact that ppid < pid so parents are swept
first (a topological sweep). It is a **finite, terminating, single-orbit pipeline**:
from seed `(cursor=0, d0=0, rest=−1)` it walks deterministically through exactly 6
states and parks at the fixed point `cursor=5, [0,1,1,2,2,2]` — the fully-built tree.
There is no cycle, no branching, no basin structure. Any diagram that shows otherwise
is fabricating.

## Ranked best → worst

1. **time_series** — Definitive. Seven stacked tracks show each `dN` latching from −1
   to its true tree depth in topological order (d1 at tick 1, d2 at tick 2, d3 at 3, d4
   at 4, d5 at 5, d0 flat at 0), cursor ramping 0→5 then flat. This IS the forest
   materialising one node per tick; reads like the docstring.
2. **morse_graph** — A clean 6-box vertical chain with full depth vectors annotated in
   each node (`[d2=-1 d3=-1 ... d0=0]` → ... → `[d2=1 d3=2 d4=2 d1=1 d5=2 d0=0]`), red
   start box, green terminal. Exactly the right topology, and it spells out the depths.
3. **reachability_tree** — Honest linear chain of 6 nodes labelled with full state
   tuples, green root → red absorbing fixed point. Title even flags "numeric: reachable
   set unbounded — capped sample," yet the result is correct and faithful.
4. **timing_diagram** — Same truth as time_series over 40 ticks, making the "settle then
   hold forever" plateau explicit. Slightly redundant with time_series but confirms
   termination.
5. **cobweb** (on cursor) — Staircase from 0 climbing to 5 and sticking on `y=x` is a
   genuinely good read of the clamped cursor ramp and its fixed point. The stray dots at
   x=6,7 (off-trajectory `f` samples) are minor noise.
6. **transition_matrix** — A near-perfect diagonal (each state → next) is the right
   shape, but it is rendered over ~60 sampled states with off-trajectory rows, so the
   real 6-state spine is buried among fabricated entries and the labels are illegible.
7. **state_graph** — Correct 6 states / 6 edges with a self-loop on the terminal, but
   the node labels overlap into mush and two nodes are flung to the bottom-left by the
   numeric layout, obscuring the simple chain.
8. **morse**/**chord_diagram** — Reads cursor transitions as a tidy ring `+1→−1→−2→−4→
   −5→+5→...`, but the negative-binned nodes are an artifact of sampling cursor over a
   ±6 range it never visits; it invents a circular flow for what is a straight line.
9. **fixedpoint_map** — Technically correct ("1 fixed point" at cursor=5, d2=1) and the
   6 sampled states sit on the right line, but it conveys almost nothing the chain
   diagrams don't, and the lone star carries the whole signal.
10. **basin_map** — "1 basin" is the correct count, but it's drawn as a uniform grid of
    identical blue squares with a meaningless ±2 cursor span and an attractor star
    floating off the data; degenerate.
11. **parallel_coords** — Most axes (d3, d4, d1, d0) are dead flat at a single value;
    the only spread is on `cursor` running −1500→+1500, values the program never
    reaches. Sampling noise dressed as structure.
12. **scatter_matrix** — 586 "sampled states" over ±2000 axes. The real trajectory is 6
    points; everything else is fabricated off-orbit sampling. Unreadable and misleading.
13. **orbit_scatter** — Seeds scattered at cursor=−1500, +2800, d2=2700 — coordinates
    the system cannot occupy. Pure ±3000-range fabrication; the actual orbit is invisible.
14. **occupancy_heatmap** — A periodic lattice of bright cells across a ±3000 grid. The
    program dwells at exactly ONE point (cursor=5); this paints dwell-mass across
    thousands of fictional cells. Worst offender.
15. **nullcline_field** — Two giant sign-regions and a vector field over a ±3500 grid,
    implying continuous dynamics with nullclines. pstree is a discrete latching sweep
    with no such field; entirely fabricated.

## Verdict

**Keepers (3):** `time_series` (the per-node latching is the program), `morse_graph`
(exact 6-state chain with depth vectors), `reachability_tree` (honest linear
forest-build chain). `cobweb` and `timing_diagram` are useful seconds.

**Drop:** every numeric-grid renderer — `occupancy_heatmap`, `nullcline_field`,
`orbit_scatter`, `scatter_matrix`, `parallel_coords`, `basin_map`. All of them sample
the Int axes over ±2000–3500, a region this program never enters, and consequently
**invent** lattices, basins, vector fields, and scattered orbits for what is a
6-state straight line that builds a tree and stops. `chord_diagram` and the lower-tier
`transition_matrix`/`state_graph` are correct in count but obscured by the same
off-trajectory sampling or label clutter.

**Notable finding:** the discrete/graph extractors (morse, reachability, time_series)
all independently agree on the exact same 6-state chain ending at `[0,1,1,2,2,2]` —
strong corroboration that the FSM is correctly constrained — while every continuous
numeric renderer fabricates structure from the hardcoded ±thousands sampling range.
This program is a crisp demonstration that the numeric viz family is unsound for
small-domain, terminating FSMs: it doesn't just look bad, it asserts attractors,
basins, and nullclines that do not exist.
