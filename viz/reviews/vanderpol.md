# vanderpol — visualization review

## What the program does

`vanderpol.ev` is a 2D difference equation carrying integer state `(state.x,
state.v)` at fixed-point scale S=1024. The transition is the discretized van der
Pol oscillator: a nonlinear damping term `(1 - x²)` feeds the velocity update,
and position integrates velocity. The origin is an **unstable fixed point** — any
nonzero seed spirals *outward* from it — while a single **limit cycle** (a closed
ring roughly ±2000 in both axes) is the global attractor. So the true behavior is:
one repeller at center, one attracting periodic orbit, every trajectory converging
onto the same ring regardless of starting point. The interesting structure is
entirely geometric and lives in the `(v, x)` plane.

## Ranked best → worst

1. **fixedpoint_map** — Cleanest, most honest single picture: a red star marks the
   unstable fixed point at the origin AND traces the limit cycle as one closed
   loop labeled "1 fixed point + 1 cycle (period [312])". This is *exactly* the
   van der Pol story, stated explicitly. Faithful and diagnostic. Keeper.
2. **occupancy_heatmap** — Bright ring on a dark field showing where the system
   dwells; the limit cycle pops as the high-occupancy band with a dim interior
   (the repelling center). Beautifully representative of "where it ends up."
3. **orbit_scatter** — Four seeds, time-colored (purple→yellow), all spiraling
   onto the same ring. Directly shows convergence-to-cycle and that the attractor
   is seed-independent. The most *narrative* view of the dynamics.
4. **phase_portrait** — Vector field + red/orange trajectory loops + the fixed
   point starred. Correct and information-rich, but the quiver arrows clutter and
   the loops read slightly busier than orbit_scatter/fixedpoint_map.
5. **scatter_matrix** — Off-diagonals reproduce the limit-cycle loop in both
   orientations; the v/x marginal histograms show the bimodal dwell of an
   oscillator. Redundant with the dedicated phase views but internally consistent.
6. **morse_graph** — Correctly condenses the reachable graph to a single recurrent
   component "cycle ×70" at the bottom with transients funneling into it. The right
   topological abstraction for a limit cycle, though the quantized grid labels are
   noisy.
7. **fixedpoint... time_series** — Shows v and x evolving smoothly over 61 ticks
   from one seed; you can see oscillatory shape building. Useful but single-seed,
   doesn't reveal the cycle/repeller structure.
8. **timing_diagram** — Same as time_series with two analog lanes; fine but
   strictly less than time_series (fewer ticks, no extra info). Redundant.
9. **state_graph** — Sampled trajectory spirals with terminal points marked;
   geometrically suggestive but axis labels are unreadable and it's a messier
   version of orbit_scatter.
10. **transition_matrix** — A quantized diagonal band (states map to near-neighbors).
    Mildly confirms locality of the flow but tells you little about the cycle;
    a continuous oscillator is a poor fit for a state-to-state matrix.
11. **chord_diagram** — Binned `state.v` into 8 arcs with flow between them. It does
    capture cyclic flow around the ring, but binning a continuous variable into 8
    nodes is a lossy, hard-to-read fit.
12. **nullcline_field** — Sign-region quadrants of (dx, dv) with a tiny cluster of
    sampled points at center. The nullcline idea is right for this system, but the
    rendering is dominated by colored sign-regions and the actual orbit is invisible.
13. **parallel_coords** — Two axes (v, x) with 726 crossing lines colored by sample
    order. A noodle tangle; for a 2-variable continuous oscillator this conveys
    essentially nothing a scatter wouldn't do better.
14. **reachability_tree** — A single vertical chain of 9 nodes (one trajectory,
    depth 8). For an unbounded continuous state the "reachable set" is meaningless
    here; it's just a relabeled time series with no branching.
15. **basin_map** — Degenerate: a uniform blue grid, "1 basin," every seed converges
    to the same cycle. Technically true but conveys zero structure — there's only
    one basin so the map is a solid field.
16. **cobweb** — Worst. Projects onto `state.v` alone (holding x) and produces a
    near-perfect `y=x` line of dots. The 1D cobweb assumption is wrong for a
    coupled 2D system; the picture is an artifact, not the dynamics.

## Verdict

**Keepers:** `fixedpoint_map`, `occupancy_heatmap`, `orbit_scatter` — between them
they name the unstable fixed point, show the limit cycle as the dwell region, and
demonstrate seed-independent convergence onto it. `phase_portrait` is a strong
fourth.

**Drop:** `cobweb` (mislabeled 1D projection of a 2D system, degenerate y=x),
`basin_map` (single basin → solid field, no information), `reachability_tree`
(single chain, meaningless for unbounded continuous state), `parallel_coords`
(2-var noodle mess). `timing_diagram` is redundant with `time_series`.

**Notable finding:** The cobweb and reachability_tree expose a generator
assumption mismatch — both force 1D / discrete-reachability framings onto a
continuous 2D oscillator and produce artifacts (a y=x line; a non-branching chain)
rather than degrading gracefully. Conversely, fixedpoint_map deserves credit for
correctly *detecting and separating* the unstable fixed point from the limit cycle
— it didn't just draw the orbit, it classified the structure.
