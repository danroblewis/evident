# Visualization review — `top`

## What the program does

`top` is a bounded top-k-by-memory accumulator run as an FSM — `top(3)` over a
fixed list of six processes. The carried **state** is a `TopK` record: a `cursor`
(index of the next process to consume) plus three sorted (pid, mem) slots
`(p0,m0)`/`(p1,m1)`/`(p2,m2)` held in descending memory order, empty slots
sentinelled at `-1`. Each tick reads `procs[cursor]`, runs an insert-if-larger
into the sorted window (`at0`/`at1`/`at2`/`keep`), shifts the tail down, evicts
third place, and advances the cursor. The behavior is **deterministic and
terminating**: a single straight-line orbit of 7 states (cursor 0→6), where m0
ratchets 4096→8192→16384, m1 fills to 8192, m2 churns (-1→2048→4096→6144), then
at cursor≥6 the board **freezes — a self-loop fixed point, no cycle**. There is
exactly one trajectory; nothing branches, nothing recurs.

## Ranked best → worst (for THIS program)

1. **morse_graph** — The truth of this program in one image: a 7-node vertical
   chain with each node labeled `cursor=N [m0= m1= m2=]`, red start, green
   terminal. You read the ratchet (m0 8192→16384), the freeze (cursor=5 and =6
   identical m0/m1, only m2 moves), and the halt directly off the labels. Exact,
   bounded, faithful.
2. **morse_graph / time_series tie — time_series** — Four stacked tracks
   (cursor, m0, m2, m1) over 8 ticks show the staircases plainly: cursor climbs
   1/tick then plateaus at 6; m0 steps and saturates at 16384; m2's late churn to
   6144 is visible. The cleanest quantitative view of the actual run.
3. **reachability_tree** — Same 7-node chain as morse but with **full state
   tuples** `(cursor, m0, m1, m2, p0, p1, p2)` on each node, depth-6, single
   path, red absorbing terminal. Confirms the determinism (one child per node)
   and shows the pids too. Slightly redundant with morse but adds the pid detail.
4. **timing_diagram** — Correct staircases for all four ints over 40 ticks,
   makes the freeze obvious (flat from tick ~6 onward). Marred only by 34 wasted
   flat ticks of padding; the run is over by tick 6.
5. **cobweb** (on `state.cursor`) — Honestly shows the cursor map as
   `x_{n+1}=x+1` stepping up the staircase to a wall at 6; the y=x diagonal and
   red orbit are correct. Limited because cursor is a trivial counter, but it
   does NOT fabricate — the points outside [0,6] are just the sampled map, the
   orbit stays in-range.
6. **state_graph** — Right topology (7 states, 7 edges, terminal flagged) but the
   node labels are **overprinted into illegible mush** — every tuple stacked on
   top of its neighbor at near-identical y. The chain shape survives; the labels
   are unreadable.
7. **fixedpoint_map** — Correctly reports "1 fixed point" and stars the terminal
   (cursor=6, m0=16384). Accurate but low-information: a scatter of 7 nearly
   invisible grey dots with one star; the morse/tree already told us this.
8. **chord_diagram** — Bins `state.cursor` into [-7,7] and draws a transition
   chord graph. The real path 0→1→...→6 partly survives as the +1→+3→+4 arcs, but
   the **negative-cursor nodes (-6,-4,-3) are pure sampling artifact** — the
   cursor never goes negative. Invents flow on a state that doesn't exist.
9. **parallel_coords** — 248 "sampled states" with axes cursor∈[-1500,1500],
   m0/m2/m1∈[0,7]. The axis ranges are nonsense for this program (cursor maxes at
   6; m's reach 16384, not 7). It's plotting the fabricated sample cloud, not the
   trajectory. Misleading.
10. **scatter_matrix** — 585 "sampled states" splattered across cursor∈[-3000,3000]
    and negative m-values. The real states (the handful of off-axis dots at m0=8192,
    16384) are buried in a hand-built grid of points the system never visits.
    Fabricated structure.
11. **orbit_scatter** — Claims "multiple seeds → attractor" and plots seeds at
    cursor≈-1500, +400, +2800 with a black drift bar. The actual orbit is the few
    dark dots near cursor=0 climbing m0; everything else is invented seed clutter
    on an axis (cursor to ±3000) the counter never reaches.
12. **occupancy_heatmap** — A regular lattice of orange/yellow blobs over
    cursor∈[-3000,-700], m0∈[0,3000]. This is the +/-3000 sampling grid rendered
    as "where the system dwells" — but the system dwells at cursor=6, m0=16384,
    which is **off the plotted range entirely**. Pure artifact; tells you nothing
    true.
13. **nullcline_field** — Sign-region quiver over cursor∈[-3500,3500],
    m0∈[-3500,4500] with arrows and dotted nullclines. Treats a discrete bounded
    counter as a continuous vector field on a huge fabricated domain. The real
    dynamics (cursor +=1 until 6, then stop) is nowhere in this picture.
14. **phase_portrait** — A vector field over ±5000 on both axes with a couple of
    tiny circle markers. The hardcoded sampling range invents a flow field across
    thousands of units the program never enters; the actual 7-point orbit is
    invisible. Despite the docstring advertising this exact view, it's the least
    representative of the bunch.
15. **transition_matrix** — **Failed to render** (no PNG produced).

## Verdict

- **Keepers (2–3):** `morse_graph` (the single most faithful + readable view —
  the labeled halting chain), `time_series` (cleanest quantitative staircases),
  and `reachability_tree` (same chain with full pid-carrying tuples, confirms
  determinism). Together they tell the whole story: deterministic 6-step ratchet,
  one trajectory, freeze at the end.
- **Drop:** every numeric-sampled renderer — `phase_portrait`, `nullcline_field`,
  `occupancy_heatmap`, `orbit_scatter`, `scatter_matrix`, `parallel_coords`, and
  `chord_diagram`. All of them inherit the hardcoded ±3000/±5000 axis sampling and
  **fabricate structure on a program whose real state space is 7 points clustered
  near the origin**. They show vector fields, basins, occupancy lattices, and
  negative-cursor nodes that have no counterpart in the actual run.
- **Notable finding:** This is a textbook case of the known numeric-sampler bug.
  `top` is a deterministic counter+leaderboard that visits exactly 7 small states
  and halts, yet `occupancy_heatmap` claims it "dwells" at cursor∈[-3000,-700]
  (off the real range), `chord_diagram` shows transitions among non-existent
  negative-cursor nodes, and `nullcline_field`/`phase_portrait` paint dense flow
  across ±thousands of units the cursor (max 6) never reaches. The graph-based
  renderers that build from the **actual reachable set** (morse, reachability,
  state_graph, fixedpoint, timing, time_series) are all correct; the renderers
  that **sample a hardcoded numeric box** are all wrong here. The split is clean
  and total. `transition_matrix` additionally failed to render.
