# scheduler — visualization review

## What the program does

`scheduler` is a fixed three-job priority scheduler run as an FSM. Its carried
state is `SchedState`: three remaining-quantum slots `r0, r1, r2` (priorities 1,
3, 2 respectively), `running` (the id of the job selected this tick, or `-1`
idle), and `clock` (elapsed quantums). The transition picks the highest-priority
ready job in fixed order job1 ▸ job2 ▸ job0, decrements that job's slot, sets
`running` to its id, and advances `clock` by one — UNLESS everything has drained,
in which case `running` latches to `-1` and `clock` freezes. With the seeded
workload (r0=1, r1=2, r2=1) the run is a short, fully deterministic, **linear**
trajectory: clock climbs 0→1→2→3→4, `running` walks 1,1,2,0 down the priority
ladder, every slot drains to zero, and the system lands on the single absorbing
fixed point `(clock=4, running=-1, r=0,0,0)` and halts. There is no cycle, no
branching, no large numbers — `running` lives in `[-1, 2]`, `clock` in `[0, 4]`.

## Ranked best → worst

1. **morse_graph** — Exact and perfectly faithful: six labeled boxes in a single
   chain showing the literal state at each clock tick (`clock=1 running=1 r0=1
   r1=1 r2=1` …), red start, green terminal. Reads as the program's actual life
   story; you can verify the priority dispatch by eye.
2. **state_graph** — Same reachable chain laid out spatially with the absorbing
   fixed point doubled and colored orange; the `(clock, running, r…)` tuples on
   each node make the dispatch order legible. The self-loop on the terminal node
   correctly conveys "halts here."
3. **time_series** — Five honest per-variable tracks on the real scale: clock
   ramping then flat, running tracing -1→1→1→2→0→-1 (the ladder), each r slot
   stepping down to 0. The single clearest "what happens over time" view.
4. **reachability_tree** — The BFS tree is just the linear chain (6 nodes, depth
   5) with the absorbing state ringed red — exact and correct. Slightly redundant
   with morse/state_graph, and the title's "unbounded — capped sample" warning is
   honest about the numeric risk it dodged here.
5. **fixedpoint_map** — Correctly scans the *reachable* set and pins exactly ONE
   fixed point (★ at clock=4, running=-1) on the true small scale. Sparse but not
   wrong; the "reachable" scope kept it from fabricating phantom fixpoints.
6. **timing_diagram** — Logic-analyzer view over 40 ticks; correct but the run
   ends at tick 4, so 90% of the plot is flat dead air. Same content as
   time_series, lower information density.
7. **cobweb** — The clock staircase (xₙ₊₁ = xₙ+1, orbit climbing above y=x) is a
   legitimate read of the clock recurrence, but clock is a trivial counter, so it
   illustrates the least interesting variable; the stray (-2,-1)/(-1,0) sampled
   dots hint at out-of-range probing.
8. **parallel_coords** — The r0/r1/r2 axes collapsing to 0 is faithful, but the
   `clock` and `running` axes span ±1500 / fabricated values, polluting an
   otherwise readable trajectory sweep.
9. **basin_map** — Degenerate: one basin over a 42-seed grid, legend admits
   `clock≈9` (an artifact of where sampling stopped, not a real attractor since
   clock never converges). Conveys only "everything drains," at the cost of
   implying a basin structure that doesn't exist.
10. **chord_diagram** — Bins `clock` into nodes ±5 and draws self-arcs; for a
    monotone +1 counter this is meaningless decoration. No transition structure
    to show.
11. **occupancy_heatmap** — **Failed**: renders "N/A — no visited states
    (transition unsat)." The generator couldn't even seed a trajectory; empty
    panel.
12. **orbit_scatter** — Fabricated. Plots seeds at running≈2700, 1500 and
    clock≈±1500/2800 — values this program NEVER produces (running ∈ [-1,2]).
    Pure artifact of the ±3000 numeric sampling.
13. **scatter_matrix** — Fabricated. The r0/r1/r2 marginals are real (0/1/2
    spikes), but every panel touching clock or running spans ±3000 with a grid of
    invented points; "204 sampled states" are mostly states the FSM can't reach.
14. **transition_matrix** — Fabricated. A 64×64-ish identity diagonal over a
    sampled grid colored by a ±5000 value scale — it shows the sampler's grid,
    not the program's 6-state transition relation.
15. **nullcline_field** — Worst. A full ±3500 × ±3500 vector field with
    orange/purple sign regions and a `running=0` nullcline, as if this were a
    continuous 2D dynamical system. The program is a discrete 6-state chain on a
    tiny scale; this invents an entire phase plane out of the hardcoded sampling
    range.
16. **phase_portrait** — Also fabricated (±4000 vector field of uniform arrows),
    and ironically the one the program's own docstring suggests. The real
    `(clock, running)` walk is 5 points in a 5×4 box; the renderer drew a
    featureless thousands-wide arrow grid instead.

## Verdict

**Keepers (2–3):** `morse_graph` is the single best diagram — exact, labeled,
and it literally narrates the priority dispatch. `time_series` is the best
quick "over time" read. `state_graph` (or equivalently `reachability_tree`) is
the best topological view of the halt. `fixedpoint_map` is a worthy fourth: it's
the only numeric-family renderer that scoped itself to the reachable set and so
told the truth (exactly one fixpoint).

**Drop for this program:** every wide-numeric-sampling renderer —
`phase_portrait`, `nullcline_field`, `transition_matrix`, `scatter_matrix`,
`orbit_scatter`, `chord_diagram`. They all draw the ±3000-ish sampling grid
rather than the program, and `nullcline_field`/`phase_portrait` go further by
inventing a continuous phase plane the discrete FSM never inhabits. `basin_map`
invents a basin; `chord_diagram` decorates a trivial counter; `occupancy_heatmap`
outright failed (unsat).

**Notable finding:** This program is a clean, near-perfect demonstration of the
known numeric-sampler bug. The state is small (running ∈ [-1,2], clock ∈ [0,4]),
fully reachable, and finite — yet six renderers fabricate structure at ±1500 to
±4000 (orbit_scatter literally plots running=2700). The contrast is stark
because the *exact-reachable-set* renderers (morse, state_graph,
reachability_tree, fixedpoint_map) sitting right next to them get it perfectly
right. The fix is unambiguous: numeric renderers should derive their axis range
from the actual reachable trajectory (as the morse/fixedpoint family does), not a
hardcoded ±3000. Secondary: `occupancy_heatmap` reporting "transition unsat"
while the morse graph clearly enumerates the transitions points at a separate
seeding bug in that renderer specifically.
