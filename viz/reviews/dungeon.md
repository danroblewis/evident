# dungeon — visualization review

## What the program does

`dungeon` is an autonomous, **nondeterministic** FSM whose carried state is
`Dungeon` = `{room ∈ Room(7-value enum), has_torch, has_key, has_treasure,
escaped (bools)}`. There is no external input. The transition is the
room-**adjacency relation**: from each room the next room is one of its
neighbours (Hall is the hub: it fans out to Kitchen/Library/Cellar/Gate/Entrance),
gated by items (Cellar needs torch, Vault needs key). Item bools are sticky
(once true, stay true): torch picked up in Kitchen, key in Library, treasure in
Vault; `escaped` latches when you reach Gate carrying the treasure. So the
reachable state space **is the dungeon map plus a monotone item-progression
lattice** — 25 reachable states, 65 transitions, terminating at Gate-absorbing
fixed points. This is fundamentally a **graph/reachability** program, not a
trajectory program.

## Ranked best → worst

1. **morse_graph** — The single best fit. It condenses the 25-state graph into
   the actual narrative: a top "cycle" SCC around Entrance/Hall, branching into
   torch-acquisition (Cellar), key-acquisition (Library), then a `[key torch
   treasure]` cycle, terminating in the `room=Gate [key torch treasure escaped]`
   absorbing node. The bracket annotations literally spell out the item
   progression. Faithful and legible; reads as the dungeon's solution path.

2. **reachability_tree** — Shows the real BFS structure from the Entrance seed:
   depth-8 tree, 20 first-discovery nodes, color = room, the green-ringed root
   and red-ringed Gate absorbing/goal nodes. You can trace Entrance→Hall→Kitchen
   (torch)→... and see the item bools flip in the tuple labels. Honest and
   informative; only loses to morse because it doesn't collapse the cycles.

3. **chord_diagram** — Genuinely good for THIS program: nodes = rooms laid in a
   ring, arc width = transition count, and Hall's huge node correctly identifies
   it as the hub with the most outgoing flow. The arcs reproduce the adjacency
   map (Hall→Cellar/Library/Kitchen/Gate/Entrance, Vault↔Cellar). The has_key
   hue is nearly all orange so that channel is wasted, but the topology read is
   excellent.

4. **state_graph** — The exact reachable graph (25 states / 65 edges) with self
   loops. Complete and faithful but rendered far too small — node labels are
   unreadable at this size and it's a tangle. Same content as morse without the
   condensation, so it's correct but hard to use.

5. **transition_matrix** — A 25×25 from-state→to-state matrix; the yellow cells
   trace the adjacency + monotone item flow, and the block structure (upper-left
   to lower-right drift) reflects the one-way item progression. Informative if
   you zoom, but the tiny tuple tick labels make it a reference artifact, not a
   glanceable one.

6. **occupancy_heatmap** — room × has_key faceted by has_treasure. The bright
   Gate-cell at (Gate, key=false, treasure=false) honestly shows where random
   walks pool (Gate is absorbing), and the treasure=true facet being near-empty
   shows treasure states are rarely sampled. Real signal, but the log-visit
   scale and sampling make it more about the random-walk sampler than the
   program's structure.

7. **orbit_scatter** — room × has_key, faceted by treasure, colored by torch,
   seed ringed. It does correctly show Vault is unreachable without
   key+torch (Vault column empty in the false facet) and that the seed sits at
   Entrance/no-key. But has_key on the y-axis with everything jittered into two
   bands is a weak layout for discrete data.

8. **basin_map** — Identifies the 5 terminal Gate fixed points and color-codes
   which basin each state falls into (the `→ (...,Gate)` legend). Genuinely
   shows the program is "all roads lead to a Gate fixed point," but the y-axis
   (has_key) and x-axis (room) plus jitter make the basins visually
   indistinguishable — you read it from the legend, not the picture.

9. **fixedpoint_map** — Correctly finds 5 fixed points, all at room=Gate
   (the absorbing exits), starred. Right answer, but the scatter is almost
   entirely overlapping dots in one row; nearly all the visual area is empty and
   the color=torch/shape=escaped encoding adds noise without separation.

10. **parallel_coords** — room→has_key→has_torch→has_treasure→escaped axes.
    You can faintly see that escaped=true only co-occurs with everything-true,
    and treasure→escaped is the only path to the right edge. But with 25
    crossing lines and a discrete room axis it's a hairball; the structure is
    there but buried.

11. **scatter_matrix** — 5×5 of mostly-binary variables. The diagonal histograms
    are the only real content (has_key skews true, escaped overwhelmingly false),
    confirming the monotone item bias. Off-diagonal panels are just dots at the 4
    corners — discrete data makes a scatter matrix degenerate.

12. **cobweb** — room(n)→room(n+1) as enum→ordinal, faceted by treasure. The
    scatter of reachable (x,y) pairs IS the adjacency relation, which is
    mildly interesting, but cobweb's whole point — staircasing a deterministic
    1-D orbit — is meaningless here: the map is nondeterministic and the "orbit"
    is a single red dot stuck at Entrance. Forced fit.

13. **time_series** — A single arbitrary 4-tick walk (Entrance→Hall→Gate) with
    4 of 5 panels dead-flat false. Picks ONE nondeterministic path and the bools
    happen to never flip — actively *misleading* about a program whose entire
    content is the branching item-collection structure.

14. **timing_diagram** — Same single trajectory over 40 ticks; room jumps to
    Gate by tick 2 and then 38 ticks of nothing, all four bool lanes flat at 0.
    Shows the seed walk reached an absorbing state and stalled, but conveys
    nothing about reachability and wastes 95% of its width.

15. **nullcline_field** — Correctly renders an explicit "N/A: purely discrete
    state (no numeric axis)" placeholder. Honest non-fit; not a failure, just
    inapplicable.

16. **phase_portrait** — Plots room(ordinal) × has_key with treasure facets and
    transition arrows. Because room is forced onto an ordinal axis the arrows
    imply a false metric ordering (Entrance<Hall<...<Gate), and the two-row
    has_key collapse hides the torch/treasure/escaped state entirely. The
    state_graph/morse already show the topology correctly; this one *distorts*
    it. Least representative.

## Verdict

**Keepers:** `morse_graph` (the solution-path condensation), `reachability_tree`
(the honest BFS structure), and `chord_diagram` (the hub-and-spoke adjacency,
with Hall correctly sized as the hub). Together they tell the whole story:
the map, the search order, and the item-gated progression.

**Drop for this program:** `phase_portrait` (false ordinal metric, distorts the
topology), `time_series` and `timing_diagram` (a single nondeterministic path
that flatlines and misrepresents a branching reachable set), `cobweb` (a
deterministic-orbit tool applied to a nondeterministic relation),
`scatter_matrix` (degenerate on binary data). `nullcline_field` is a correct
N/A and should stay as a placeholder, not a failure.

**Notable finding:** the graph-family views (morse / reachability /
state_graph / chord / transition_matrix) all agree on the same structure —
Hall hub, item-gated branches, 5 Gate-absorbing fixed points — which is strong
cross-validation that the adjacency + sticky-item encoding is correct. The
trajectory-family views (time_series, timing, phase_portrait) each pick ONE
arbitrary path and thereby paint a flatlined, room-only picture that
*contradicts* the rich structure the graph views reveal — a vivid demonstration
that single-orbit visualizations are the wrong tool for a nondeterministic
reachability program.
