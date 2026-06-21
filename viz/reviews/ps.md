# ps — visualization review

## What the program actually does

`ps` is a **utility/accumulator**, not a dynamical system. A cursor walks a fixed
6-element process list one record per tick. Each `Proc` is classified by an Int
scheduler code (0=Running, 1=Sleeping, 2=Disk, 3=Zombie), the matching counter
is bumped, and a running top-by-memory accumulator (`max_mem` + `max_pid`) is
updated. The carried state is a 7-field `Stats` record: `cursor` (a monotone
0→6 counter), four histogram counters (`running`, `sleeping`, `disk`, `zombie`),
and the running max pair. Behavior is a **single, deterministic, monotone
trajectory** of length 6 that freezes into one absorbing fixed point once
`cursor ≥ 6`. There is no branching, no cycle, no search — a one-shot scan that
terminates. The reachable "phase space" is a straight line; there is nothing to
fold, oscillate, or basin-partition. The final tally is
`running=2, sleeping=2, disk=1, zombie=1, max_mem=16384, max_pid=104`.

## Ranked best → worst

1. **time_series** — The most quantitatively honest view. Six stacked,
   importance-ordered panels over 8 ticks: `cursor` ramps 0→6 linearly, `max_mem`
   steps -1→4096→8192→16384 then flatlines, and each counter increments exactly
   at the tick its category is consumed, then everything freezes. You can read
   the entire histogram-fill and the absorbing plateau directly. Faithful.
   **Keep.**

2. **morse_graph** — A clean 7-node vertical chain, initial state outlined red
   (`cursor=0 … max_mem=-1`), terminal fixed point outlined green
   (`cursor=6 … running=2`), each box spelling the full tally. This *is* the
   program: a linear scan absorbing into a frozen final count, every intermediate
   legible. Faithful and complete. **Keep.**

3. **timing_diagram** — Same staircase as time_series but run to 40 ticks, which
   makes the "freeze after tick 6" plateau emphatic — good confirmation the
   fixpoint is truly absorbing. All six lanes are `[int]` analog traces (no
   bools/enums in this state). Slightly redundant with time_series; the long flat
   tails waste width. Faithful, useful as secondary confirmation.

4. **reachability_tree** — A correct degenerate tree: 7 nodes, depth 6, a single
   straight chain (root green, absorbing node red), labels carrying the full
   tuple. Truthful about the linear non-branching structure, but says the same
   thing morse_graph does. Honest but redundant.

5. **state_graph** — The 7-state reachable chain with the terminal self-loop drawn
   as the orange fixed point. Correct in substance, but node labels collide into
   an unreadable smear along the bottom row (`(5, 1, 16384, 104…` overlaps the
   next node). The morse_graph renders the same graph far more legibly.

6. **cobweb** — Cobweb on `state.cursor` (other fields held). Honestly shows the
   `x → x+1` staircase climbing off `y=x` and parking at 6 — correctly conveying
   "cursor just counts up to the wall." But a cobweb is built to reveal
   convergence/oscillation of a 1-D map; here there's nothing to reveal (a ramp),
   and it sees only 1 of 7 state fields. Marginal.

7. **chord_diagram** — Bins `cursor` into nodes on [-7,7] and draws the transition
   flow. The real +1 hops appear (+1→+3→+4, etc.), but binning a monotone index
   into a circular layout is a forced fit, and the negative-bin nodes (-6,-4,-3)
   are sampling artifacts the program never produces. Reads as structure where
   there is only a line.

8. **fixedpoint_map** — Technically correct: it finds the 1 real fixed point and
   stars it at `cursor=6, zombie=1`. But the plot is ~95% empty whitespace with
   the marker buried under the legend; the 7 sampled states are nearly invisible.
   The one true fact is stated better by morse_graph's green node. Low value.

9. **parallel_coords** — 248 swept states fanned across 6 axes, almost all yellow
   (late sample order). The real 7-state path is buried in the sweep clutter; the
   `max_mem` axis shows the 0→8 jump but the rest is noise. Sweeping a
   deterministic finite scan invents states it never visits.

10. **scatter_matrix** — A 6×6 grid over 585 *sampled* states. The real
    trajectory is 7 points; the rest is the unconstrained sampling envelope,
    including negative `cursor`/`zombie` around ±2500 that the program can never
    reach. The actual behavior is invisible; diagonal histograms spike at 0 but
    the off-diagonal panels are pure sampling noise. Misleading as a portrait.

11. **orbit_scatter** — Off-trajectory seeds smear to `cursor ≈ -1500`, a region
    the monotone scan never enters. The genuine run is one tight cluster near the
    origin, lost among artifacts. Seed sweep is meaningless for a deterministic
    terminating fold.

12. **occupancy_heatmap** — A tiled yellow lattice over `cursor ∈ [-3000,-700]`,
    `zombie ∈ ±3000` on a black field. The program "dwells" at exactly one point;
    this shows a sampling grid, not occupancy. Entirely unrepresentative.

13. **phase_portrait** — Uniform yellow rightward arrows on the left, a solid wall
    of red "fixed point" stars on the right of (cursor, zombie). The stars are
    everywhere because every off-trajectory probe looked locally stationary —
    this says nothing about the program. Visually striking, informationally empty.

14. **nullcline_field** — Dense black dots over a pink field with an illegible
    overlapping legend and one blue dashed vertical line near cursor≈0. A
    continuous vector-field/nullcline abstraction applied to a discrete
    terminating counter — meaningless. The worst forced fit.

15. **basin_map** — Claims "3 basins" colored across a (cursor, zombie) seed grid.
    Actively misleading: this program has exactly ONE attractor (the frozen
    tally). The three "basins" are artifacts of seeding off-trajectory states the
    scan logic resolves differently — it manufactures structure the program does
    not have.

16. **transition_matrix** — **Did not compute.** Error box: "N/A for int state:
    could not build state set: 'state.max_pid'". The unbounded `max_pid` ID field
    defeats discrete-state enumeration. A legitimate, honest "not applicable" —
    it failed loudly instead of fabricating a matrix.

## Verdict

- **Keepers (2–3):** `time_series` (the histogram building up, quantitative) and
  `morse_graph` (the canonical linear scan + labeled absorbing fixed point), with
  `timing_diagram` a distant third (confirms the freeze).
- **Drop for this program:** the entire dynamical-systems battery —
  `basin_map`, `nullcline_field`, `phase_portrait`, `occupancy_heatmap`,
  `orbit_scatter`, `parallel_coords`, `scatter_matrix`, plus `chord_diagram`,
  `fixedpoint_map`, and `cobweb`. None earn their place on a one-shot
  terminating accumulator; most actively mislead.
- **Notable finding:** `basin_map` **invents 3 attractor basins for a program
  with exactly one**. This is the picture revealing a viz-generator over-reach,
  not a program property — the "basins" come from feeding the scan logic states
  it would never reach. The same tell appears across orbit_scatter, occupancy,
  phase_portrait, and scatter_matrix: all wander into **negative cursor values**,
  impossible for a monotone `cursor ∈ [0,6]`, proving the sampler ignores the
  program's actual reachable set. Separately, `transition_matrix` honestly bailed
  on `max_pid` (an unbounded ID, not enumerable state) — the correct failure.
