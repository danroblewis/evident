# find — visualization review

## What the program does

`find` is a **live directory search**: a BFS/DFS-agnostic traversal over a fixed
6-node tree, modelled as a constraint problem. The carried state is a `Tree`
record holding a per-node frontier status (`s0..s5 ∈ {Unseen, Pending, Visited}`),
a per-node match-latch (`m0..m5 ∈ Bool`), the node `current` popped this tick, and
a running `n_matched` count. Each tick nondeterministically pops a `Pending` node,
marks it `Visited`, pushes any `Unseen` children of a directory to `Pending`, and
latches/increments a match when the popped node is a `.ev` file (nodes 3, 4). The
search seeds with only the root pending and terminates with `current = -1` once the
frontier empties — a **monotone progress** machine: statuses only ever advance
Unseen→Pending→Visited, `n_matched` only rises (here to 2), and the terminal state
is an absorbing fixed point. The pop is genuinely nondeterministic, so the
*reachable set* fans out over all legal visit orders, not a single trajectory.

## Diagrams, best → worst

1. **time_series** — The clearest possible read of this program. One trace per
   state field, importance-ordered: `state.current` climbs 0→4 then drops to −1
   (done); `n_matched` steps 0→1→2 and latches; each `s*` lane climbs the
   Unseen→Pending→Visited staircase exactly once and never reverses; `m3`/`m4`
   flip false→true at the right ticks; `s5` (a node with no parent reached in this
   order) stays flat Unseen. This *is* the traversal, frame by frame. Faithful and
   complete. **Keeper.**

2. **timing_diagram** — Same story as time_series with better type-aware lanes
   (enum lanes green, bools as digital fills, ints analog) and a 40-tick horizon
   that makes the absorbing terminal obvious (everything flat after tick ~5). The
   monotone staircases and the `current → −1` settle are unmistakable. Slightly
   redundant with time_series but the cleaner encoding makes it the better
   single-frame artifact. **Keeper.**

3. **reachability_tree** — The most *structurally* honest picture of the search:
   a BFS tree of 18 reachable states, depth 6, branching where the nondeterministic
   pop forks the visit order. It shows the thing time_series can't — that there are
   *many* legal orderings converging on one absorbing/goal leaf (red ring). Node
   labels overlap badly and are unreadable, but the shape (fan-out then funnel to a
   single goal) reads correctly. **Keeper for structure.**

4. **state_graph** — A sampled 18-state / 25-edge reachable graph colored by `s2`.
   Conveys the real branching of visit orders and flags the terminal as a fixed-
   point ring (top-left, orange self-loop). The node labels are a smeared mess of
   overlapping tuples, but the topology — sources at bottom, sink at the ring — is
   genuine. Informative if you squint; the label collision is the only flaw.

5. **morse_graph** — Condensation of the reachable transition graph into a clean
   top-to-bottom DAG of the 6 "current=k" strata. This is actually a nice summary:
   it shows the traversal as ordered levels (current=0 → ... → current=-1) with the
   dominant `s2` shading. Faithful to the monotone layering. Labels tiny but the
   skeleton is right.

6. **parallel_coords** — All 18 reachable states across 10 axes colored by `s2`.
   You can see the correlated structure (current/n_matched low ↔ early states; the
   `s*` axes all pinned at Unseen/Pending/Visited tiers). It's busy but it does
   honestly show the joint reachable set and that `s5` stays Unseen across the
   board. Marginally useful, more decorative than diagnostic.

7. **chord_diagram** — Reduces transitions to flow among `s2`'s three values
   (Unseen→Pending→Visited), arc width = transition count. It correctly captures
   the *one-way* status flow (thin Unseen→Pending, thick Pending→Visited, no back
   edges) — which is the monotonicity invariant. Narrow (only one field's lifecycle)
   but not wrong.

8. **fixedpoint_map** — Finds the 1 fixed point (current=−1, n_matched=2, all
   Visited) correctly and stars it. But two of three facets (s5=Pending, s5=Visited)
   are entirely empty, and the s5=Unseen panel is a sparse scatter of reachable
   `(current, n_matched)` points. The fixed point is real and useful; the faceting
   wastes 2/3 of the canvas.

9. **cobweb** — Faceted "f(current)" map. The s5=Pending panel actually shows a
   coherent orbit (0→5 staircase along y), which is a happy accident that resembles
   the pop sequence. But `current` is a *nondeterministic pick*, not a deterministic
   function of itself, so a cobweb is the wrong model — the scatter of dots at y=0/5
   is the encoder forcing a 1-D map onto a relational pop. Misleading framing.

10. **scatter_matrix** — 10×10 grid of pairwise scatters over 210 sampled states.
    Mostly degenerate: the `s*`/`m*` panels are 2–3 discrete rows of dots, and the
    `current`/`n_matched` panels span ±2500 because the sampler drew *unreachable*
    states with wild `current` values. Tiny, redundant, no insight.

11. **basin_map** — Three identical faceted panels of a uniform dot grid, all in
    "basin 0" converging on the one attractor. Technically correct (one basin) but
    visually empty — it's a featureless lattice with a 1-line legend doing all the
    work. The faceting by s5 is pure redundancy (three copies of the same grid).

12. **phase_portrait** — `current` vs `n_matched`, faceted by `s5`. Dominated by a
    red horizontal line at y≈0 and a "fixed point" star way out at x≈2500 — both
    artifacts of the sampler exploring `current` into the thousands, which is
    **out of the program's −1..5 domain**. The real dynamics live in a tiny x∈[−1,5]
    sliver that's invisible at this scale. Misleading; the axes are dominated by
    junk states.

13. **transition_matrix** — Did render, but it's an error card: *"N/A for bool,
    enum, int state: could not build state set: 'state.m0'."* Honest failure (the
    14-field mixed state has no finite enumerable index), but zero information.

14. **nullcline_field** — Worst. A sign-region vector field of (dcurrent,
    dn_matched) over current∈[−3500,3500], n_matched∈[−3500,3500]. The program's
    `current` only ever lives in [−1,5] and `n_matched` in [0,2]; this plots a
    continuous flow field over a 7000×7000 phantom domain that the program never
    visits. It treats a discrete nondeterministic search as a smooth 2-D ODE. The
    big black blob in the center is the only region with real data, crushed to a
    dot. Completely misrepresents the program.

## Verdict

**Keepers:** `time_series` (the definitive per-field traversal read),
`timing_diagram` (cleaner type-aware twin), and `reachability_tree` (the only one
that shows the nondeterministic fan-out of visit orders, the program's defining
structural feature). `morse_graph` / `state_graph` are honorable mentions for
topology if labels were legible.

**Drop for this program:** `nullcline_field` and `phase_portrait` (both invent a
±3500 continuous domain the program never enters — actively misleading),
`basin_map` and `scatter_matrix` (degenerate / redundant), `cobweb` (wrong model:
a nondeterministic pop is not a self-map), and `transition_matrix` (renders only an
error card).

**Notable finding:** the continuous-dynamics views (`nullcline_field`,
`phase_portrait`, `scatter_matrix`) expose a **sampler bug**, not a program bug —
the state sampler draws `current`/`n_matched` values in the thousands, far outside
the program's actual −1..5 / 0..2 reachable range, because those Int fields aren't
constrained to their domain when sampled in isolation. Every numeric-axis viz is
then dominated by phantom unreachable states, and the real dynamics collapse to an
invisible sliver. The discrete reachable-set views (time_series, reachability_tree,
state_graph) sidestep this by walking actual transitions, which is exactly why they
are the faithful ones for a search/frontier program like this.
