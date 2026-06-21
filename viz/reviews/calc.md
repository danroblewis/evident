# calc — visualization review

## What the program does

`calc` is an RPN (postfix) evaluator run as an FSM. The carried state is an
operand **stack**: a cursor `pos` into the input stream, a `depth` (live count),
four Int slots `s0..s3` (capacity 4), an `ok` latch (false on stack underflow),
and a `done` latch (set when the `TEnd` sentinel is consumed). The transition is
a stack machine: a `Num(n)` token pushes `n` (depth +1); a binary op pops two
operands, combines them, and pushes the result (depth −1). The input is a fixed
representative expression `3 4 + 5 *  TEnd`, which evaluates `(3+4)*5 = 35`. The
run is a **straight 8-state line, not a cycle**: depth climbs 0→1→2 on the
numbers, dips on each operator (2→1, 2→1), s0 walks 0→3→7→35, then `done` latches
true at pos=6 and the state freezes at a single absorbing fixed point
`(pos=6, depth=1, s0=35, done=true)`. It always halts; it never loops.

## Ranked best → worst (for THIS program)

1. **morse_graph** — Near-perfect: the linear evaluation trace, each node labeled
   `pos / s0 / s1 / depth`, walking s0 from 0 to 35 and ending in the green
   absorbing fixed point. You can read the entire computation off it.
2. **time_series** — The honest run: s0 stair-steps to 35, depth saws 0→1→2→1→2→1,
   `done` flips at tick 6, `ok` stays true. Faithful and legible (it surfaces the
   real values; the docstring's "ok stays true" is confirmed).
3. **fixedpoint_map** — Correctly finds the ONE fixed point at `(pos=6, s0=35)`
   and the ok=true facet traces the genuine s0 trajectory 0→3→3→7→7→35. Pinpoints
   the answer.
4. **reachability_tree** — Clean linear chain of 8 nodes, root highlighted, ending
   in the red absorbing/done state. Same story as morse_graph, slightly less label
   density.
5. **timing_diagram** — Faithful per-variable digital/analog lanes; clearly shows
   the system settling to its fixed point by tick 6 and staying there for 40 ticks.
6. **state_graph** — Right topology (8 states, 9 edges, linear, one terminal ring)
   but the node labels overlap into an unreadable smear near the top.
7. **parallel_coords** — Uses only the 8 reachable states (faithful), but the
   crossing orange/blue/grey lines over pos/ok/s0/s1/depth/done/s2 are noisy and
   hard to extract a narrative from.
8. **cobweb** — Honest about pos's dynamics (pos just increments → staircase on
   y=x), but pos alone is a trivial counter; tells you nothing about the stack.
9. **chord_diagram** — Degenerate: two boolean `ok` nodes with a single arc.
   Technically correct, near-zero information.
10. **basin_map** — FABRICATED. Invents "basin 1: st.pos≈4000, st.depth≈3994
    (cycle)" at pos≥7 — a runaway that doesn't exist. The real input is 6 tokens;
    pos saturates at 6. The +/-3000 Int sampling manufactured a phantom attractor.
11. **scatter_matrix** — FABRICATED axes: st.pos sampled ±2500, st.depth to 400.
    The real states occupy a tiny corner; the diagonal "trends" are sampling
    artifacts, not program structure.
12. **orbit_scatter** — FABRICATED: seeds at pos = −1500, +2700, +3200 — far
    outside the program's actual domain `pos ∈ {0..6}` — producing fake
    attractors.
13. **occupancy_heatmap** — FABRICATED: pos axis −3000..+3000 with s0 pinned at
    ~4 (a sampling artifact value, not the real result 35). Says nothing true.
14. **nullcline_field** — FABRICATED continuous-flow field over ±3500 with
    sign-region shading and arrows. calc is a discrete stack machine, not a
    vector field; this is meaningless here.
15. **phase_portrait** — FABRICATED/empty-looking: a sparse grid of stray glyphs
    over pos ±6000, s0 ±4000, faceted by ok. The real trajectory (a short line
    in a small box) is invisible against the giant fabricated axis range.
16. **transition_matrix** — MISSING (no `transition_matrix__calc.png` rendered).

## Verdict

**Keepers (3):** `morse_graph`, `time_series`, `fixedpoint_map`. Together they
tell the whole story — the labeled evaluation trace, the honest variable
time-courses, and the single fixed point that IS the answer (s0=35). `reachability_tree`
and `timing_diagram` are solid runners-up.

**Drop:** all six numeric-sampling renderers — `basin_map`, `scatter_matrix`,
`orbit_scatter`, `occupancy_heatmap`, `nullcline_field`, `phase_portrait` —
fabricate structure for this program. `chord_diagram` is degenerate.

**Notable finding:** This program is a textbook case of the hardcoded ±3000 Int
sampling bug. calc halts on a 6-token input with pos ∈ {0..6}; the numeric
renderers sample pos/s0/depth into the thousands and CONFIDENTLY INVENT a
"basin 1 … (cycle)" with `depth≈3994` and attractors at pos ≈ ±3000. A reader
trusting basin_map/orbit_scatter would conclude calc has a runaway divergent
cycle — the exact opposite of its real behavior (a clean, always-halting linear
evaluator). The exact, reachable-set-based renderers (morse, reachability_tree,
fixedpoint, time_series) get it right precisely because they don't sample blind.
