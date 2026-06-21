# Visualization Review — `ls`

## What the program does

`ls` is a directory-lister utility modeled as a one-shot scan. Its carried state is a
`Summary` record: `cursor` (the index of the next entry), plus four accumulators —
`total_size`, `count`, `largest`, `largest_id`. A cursor walks a *fixed* 6-entry
listing one entry per tick; each tick folds the entry's size into the running total,
bumps the count, and updates the largest-file accumulator. Once `cursor ≥ 6` the
summary freezes (a single absorbing fixpoint → halt). There is **no cycle, no basin,
no recurrence** — it is a strictly monotone 7-state march from `(0,0,0,-1,-1)` to
`(6, 81408, 6, 65536, 3)` and then it stops. The honest behavior is a finite, linear,
terminating accumulation. The *real* reachable values are tiny: count 0–6, largest in
{−1, 1024, 8192, 65536}, total ≤ 81408.

## Ranked best → worst

1. **morse_graph** — The truth, distilled: a 7-box vertical chain, red start
   (`count=0, largest=-1`), green absorbing terminal (`count=6, largest=65536`), each box
   annotated with the running largest. Exact, no fabrication, reads as "scan and halt."

2. **time_series** — Plainly shows what `ls` *is*: `count` ramps 0→6 then flatlines;
   `largest` steps 0→1024→8192→65536 then flatlines. The 65536 jump at tick 4 (data.bin)
   is unmistakable. Anyone reading this understands the program in five seconds.

3. **reachability_tree** — A straight 7-node chain, depth 6, green root, red absorbing
   leaf, each node labeled with the full state tuple. Faithful and richer than morse
   (carries total_size too). Header even *admits* "numeric: reachable set unbounded —
   capped sample," which is the honest caveat the other numeric renderers omit.

4. **timing_diagram** — Same monotone-then-flat story as time_series over 40 ticks,
   making the freeze-after-tick-4/6 obvious. Slightly redundant with time_series but
   correct and legible.

5. **state_graph** — Correct topology (7 states, 7 edges, blue→orange terminal) but the
   layout is broken: nodes collide in the bottom-left, labels overprint
   (`8192, 1, 9216)` smeared over `1024, 0, 1024)`), and the count axis is unused.
   Right answer, near-unreadable rendering.

6. **fixedpoint_map** — Correctly reports **1 fixed point** and stars it at
   `(6, 65536)`. But the 7 real sampled states are washed out near-invisible against a
   60000-tall y-axis, and the legend overlaps the data. Right count, weak presentation.

7. **cobweb** — The staircase up the `count` map is genuinely the n→n+1 increment, but
   it extends to negative `count` (−3, −2…) which this program never visits, and the
   "orbit" never terminates at 6 — it implies the counter keeps climbing forever, which
   is false. Half-true.

8. **parallel_coords** — Only two axes (count, largest); the real run is a handful of
   lines, but "248 states · trajectory sweep" means it's been padded with fabricated
   off-trajectory states (count down to −1500, largest to 30613). Structure invented.

9. **chord_diagram** — Bins `state.count` to range [−7, 7] and draws transition arcs
   among −6, −4, −3, −1, +1, +3, +4, +6. The program's count is 0–6 and strictly
   increments by 1; negative count bins and the multi-hub arc structure are pure
   artifact of sampling the +/-range. Misleading.

10. **scatter_matrix** — "210 sampled states." The largest-histogram correctly shows
    mass at small values + a spike at 65536, but `count` is spread −3000…+3000 across
    fabricated states the FSM never reaches. The one true signal is buried in noise.

11. **orbit_scatter** — Titled "multiple seeds → attractor" and draws 4 red seed rings
    at count = −1500, +400, +2800 with largest up to 2700 — **none of which are real
    states**. It invents an attractor for a program that has no attractor, only a halt.
    Actively misleading.

12. **occupancy_heatmap** — "numeric attractor: where the system dwells," a scatter of
    hot cells across a −3000…+3000 × 0…3000 grid. The program dwells at exactly ONE
    cell (the frozen summary). This is the +/-3000-sampling bug fabricating a dwell
    structure from nothing.

13. **nullcline_field** — A dense −2500…+3500 × −2000…11000 field of red/blue
    sign-regions and diamond nullclines. `ls` is a discrete counter with no vector
    field, no nullclines, no continuous dynamics. Total fabrication — pure sampling
    artifact, reads as a chaotic system.

14. **phase_portrait** — The worst. A full +/-4000 quiver field with a red "fixed point"
    star at `(~3000, ~3000)` and a second star at origin — invented attractors in a
    region the program never enters. The real trajectory (count 0–6) is a single pixel
    near the origin, invisible. Maximally misleading for a counter-that-halts.

## Missing renders

`basin_map` and `transition_matrix` did **not** render (no PNG). Given the +/-3000
fabrication pattern, basin_map would almost certainly have invented basins of
attraction this program does not have; its absence is no loss.

## Verdict

**Keepers (3):** `morse_graph`, `time_series`, `reachability_tree`. Between them they
tell the entire honest story — a finite linear scan, the 65536 largest-jump at the big
file, a single absorbing terminal, and they label the actual reachable tuples.
`timing_diagram` is a fine fourth if a "held flat after halt" view is wanted.

**Drop for this program (6):** `phase_portrait`, `nullcline_field`,
`occupancy_heatmap`, `orbit_scatter`, `chord_diagram`, `parallel_coords` — every one of
them samples the hardcoded +/-3000 axes and **fabricates** continuous-dynamics structure
(quiver fields, nullclines, dwell cells, attractor seeds, negative-count transition
hubs) that this strictly-monotone, terminating counter does not possess.

**Notable finding:** `ls` is a clean demonstration of the known +/-3000 numeric-sampling
bug. The program's true state space is 7 reachable points with `count ∈ [0,6]`, yet six
renderers draw rich attractor/basin/nullcline structure spanning thousands of units —
inventing a chaotic dynamical system out of a directory lister that just counts to six
and stops. The graph-based renderers (morse, reachability, state_graph, fixedpoint)
that walk the *actual reachable set* get it right and even self-report the cap;
the field/scatter renderers that blindly sample the axis range get it spectacularly
wrong.
