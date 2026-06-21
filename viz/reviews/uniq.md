# uniq — visualization review

## What the program does

`uniq` is a run-length collapse modeled as an FSM over a 5-field record
`UniqState` (`cursor`, `prev_line`, `run_count`, `out_count`, `line_no`). A cursor
walks a fixed 6-element line list `[10, 10, 10, 20, 30, 30]` one record per tick.
Each line id is compared to `_state.prev_line`: a match grows `run_count`, a change
emits the previous run (`out_count + 1`) and resets `run_count` to 1. Once
`cursor ≥ 6` the accumulator freezes — the system halts at a single fixpoint. The
*entire* reachable behavior is a 7-state linear chain: `cursor` 0→6, `run_count`
sawtoothing 0,1,2,3,1,1,2, `out_count` 0,0,0,0,1,2,2. There is no cycle, no basin,
no multi-attractor structure — it counts to the end of a list and stops.

## Ranked best → worst

1. **morse_graph** — the single best. Exact reachable chain, 7 states / 7
   transitions, each box carrying the *real* `[prev_line run_count out_count]` tuple,
   start (red) and absorbing fixpoint (green) marked. This IS the program.
2. **time_series** — all four numeric fields stacked, importance-ordered; you can
   read the sawtooth in `run_count` (the 3→1 snap at tick 3) and the `out_count`
   step directly. Faithful and instantly legible.
3. **reachability_tree** — same true 7-node linear chain as morse, with full state
   tuples per node and a clean root→absorbing marking. Slightly redundant with
   morse but equally exact.
4. **timing_diagram** — correct 40-tick analog traces; clearly shows freeze-after-
   tick-5. Wastes ~85% of width on the flat frozen tail, hence below time_series.
5. **fixedpoint_map** — honest: plots the 7 reachable states and stars the one real
   fixpoint `(6, 30)`. Sparse but correct; cursor/prev_line axes are odd choices
   (the fixpoint is defined by cursor=6, not prev_line).
6. **cobweb** — `cursor` increments by 1 each tick until it sticks at 6; the
   staircase-then-rest is genuinely correct, though `cursor` is a trivial counter so
   the cobweb tells you little you didn't already know.
7. **morse**-adjacent aside — none.
8. **chord_diagram** — binned `cursor` transitions; the +1→+3→+4 / -6→-4→-3 arcs are
   an artifact of the binning and the fabricated negative-cursor samples, not real
   behavior. Misleading nodes (+6, -6 never occur in the real scan).
9. **parallel_coords** — 248 swept states form a pretty funnel, but the `run_count`
   axis spans to 120 and `cursor` to ±1500 — values the program never reaches. The
   shape is the sampler, not the program.
10. **scatter_matrix** — 585 sampled states; `run_count` to 400, `cursor`/`prev_line`
    to ±3000. The diagonal "trend" in cursor×run_count is pure sampling-grid
    fabrication. Useless for this program.
11. **orbit_scatter** — seeds at `prev_line` 1500 and 2700, `cursor` ±2800; invents
    an attractor and trajectories in a state space the program never enters.
12. **nullcline_field** — sign-region quiver over `cursor`/`prev_line` ∈ ±3500 with a
    fabricated "dprev_line nullcline". The program has no continuous vector field;
    this is fiction.
13. **occupancy_heatmap** — claims to show "where the system dwells" but the x-axis is
    `cursor ∈ [-3000, -700]`. The real cursor is 0–6. Completely off the real
    support; the bright cells are sampler residue at prev_line=10.
14. **basin_map** — "1 basin on 252-seed grid", attractor reported at
    `cursor≈7, prev_line≈33, run_count≈0` — none of which is the real fixpoint
    `(6, 30, 2)`. Fabricates a uniform basin over `cursor ∈ [-1, 7], prev_line ∈
    [-5, 35]`. Worst offender: invents both the basin and a wrong attractor.
15. **phase_portrait** (FAILED INTENT) — the canonical viz the program's own docstring
    advertises (run_count × out_count sawtooth) instead renders a
    cursor × prev_line numeric vector field over ±4000 carpeted in red attractor
    stars. Zero connection to the advertised sawtooth or to actual behavior.

**state_graph did not render** — no `state_graph__uniq.png` in the gallery.

## Verdict

**Keepers: morse_graph, time_series, reachability_tree.** All three are exact,
reachable-set-based, and carry the true run-length numbers — together they fully
characterize this program (the state chain + the per-field sawtooth). timing_diagram
is a fine fourth if you want to *see* the halt.

**Drop for this program:** every numeric-sampling renderer — basin_map, occupancy_
heatmap, orbit_scatter, scatter_matrix, parallel_coords, nullcline_field, chord_
diagram, and especially the phase_portrait. They sample Int axes over a hardcoded
~±3000–4000 range the program never enters and consequently fabricate basins,
attractors, vector fields, and trends out of pure sampler residue.

**Notable finding:** the most damning artifact is that `basin_map` reports an
attractor at `cursor≈7, prev_line≈33, run_count≈0` while the program's *only* real
fixpoint is `(cursor=6, prev_line=30, run_count=2, out_count=2)` — confirmed exactly
by morse_graph and reachability_tree on the same gallery. The numeric renderer not
only invents structure, it reports a *wrong* attractor that contradicts the exact
renderers sitting right next to it. And the program's headline viz (phase_portrait,
the one its docstring tells users to run) is the worst of the lot — it shows a
fabricated cursor×prev_line field instead of the run_count×out_count sawtooth it
promises.
