# brackets — visualization review

## What the program does

`brackets` is a balanced-bracket validator implemented as a pushdown automaton run
as an FSM — a pushdown squeezed into fixed-width difference-equation state. The
carried state is a **parser stack**: a cursor `pos`, a pushdown `depth` (Nat), four
typed slots `s0..s3` (capacity-4 stack of `Open` kinds), an `ok` validity flag, and
a `done` end-of-stream flag. Each tick reads one token from the fixed balanced input
`( [ ] { } ) BEnd`, pushes on an opener (depth +1, write `slot[depth]`),
pops-and-type-checks on a closer (depth −1 if the top matches, else latch
`ok=false`), and freezes once `done`. The actual run is a single non-branching
trajectory: `pos` ramps 0→7 then saturates, `depth` traces 0→1→2→1→2→1→0, `ok`
stays true (the input is balanced), `done` latches at pos=7. **By design the post-EOF
state is under-constrained** — once `done`, only `pos`/`depth`/`ok`/`done` are pinned;
the now-dead slots are free, so the reachable graph fans out into a small
non-deterministic cluster. No real cycles, no search, no equilibrium: a finite stream
that runs to a frozen, partially-free terminal.

## Diagrams, best → worst

1. **time_series** — Best. Seven stacked lanes over 13 ticks: `pos` ramps then
   saturates at 7, `depth` shows the exact balanced 0-1-2-1-2-1-0 push/drain profile,
   `ok` holds true, `done` flips at tick 7. Faithful to the whole run — and the `s0`
   lane exposes the under-constraint: after `done`, `s0` wanders OSquare→OCurly→ORound,
   rewriting unread stale slots. Keeper.
2. **timing_diagram** — Same story, cleaner: digital lanes for `ok`/`done`, analog for
   `pos`/`depth`, enum lanes for slots. The depth pulse and done-edge read at a glance.
   The 40-tick window over-extends the 13-tick run but the saturation is honest.
3. **morse_graph** — Condensation: a clean linear chain pos=0..6 then a "cycle ×3"
   SCC at pos=7. That block isn't a real cycle — it's the post-EOF non-determinism
   collapsed into one absorbing cluster, i.e. the deliberate under-constraint. Faithful
   and diagnostic once you read the caveat; bottom labels are cramped.
4. **reachability_tree** — A single vertical spine pos=0..7 that fans into three leaves
   at pos=7 — the post-EOF free-slot branch made visible. Correct, but largely
   redundant with morse_graph/state_graph.
5. **state_graph** — The reachable depth-vs-pos staircase climbing then draining to a
   terminal with a self-ring. Structurally right, but the node labels overprint into an
   illegible smear at pos=7, so it loses to the views above.
6. **chord_diagram** — `s0` transition flow, all arcs orange (ok=true), confirming the
   run never fails. A compact "it stayed valid" summary, but only the bottom slot — it
   misses the depth dynamics. Secondary.
7. **parallel_coords** — 13 reachable states across all 7 axes; readable enough to
   confirm `depth ≤ 2`, `ok` almost always true, and `s2` always ONone. Informative but
   a tangle of crossings to read.
8. **scatter_matrix** — Misleading. Samples 192 states with `pos` and `depth` ranging
   ±2500–3000 — the unbounded Z3 Int sampling domain, not the trajectory. The genuine
   signal (ok almost always true, s2 constant ONone) is drowned by sampled noise.
9. **cobweb** — `pos(n+1)=f(pos(n))` is the trivial +1 staircase against `y=x`, faceted
   four times by `s2` (which never changes) into identical panels. Iterates the most
   boring variable; says nothing about the stack. A forced fit.
10. **fixedpoint_map** — Titled "no fixed points / short cycles found"; three of four
    `s2` facets are empty axes. Honest but near-empty — and it fails to find the
    recurrent post-EOF cluster that morse_graph/reachability_tree both caught.
11. **basin_map** — Degenerate: a uniform blue grid, "1 basin", faceted four times
    identically over a `depth ∈ [−1,3]` × `pos ∈ [0,8]` field that treats the bounded
    stack as a continuous plane. One absorbing region, no structure.
12. **phase_portrait** — A `depth`-vs-`pos` vector field over ±4000, faceted by `s2`,
    all panels identical arrow lattices. Treats a 7-token stream as a continuous flow;
    no trajectory, no meaning. (The docstring even advertises
    `phase-portrait --axes state.depth,state.ok`; the auto-axes pick pos×depth and the
    result is noise.)
13. **nullcline_field** — Worst that rendered. A dense black dot-lattice over ±3500 in
    both axes with two vertical red nullclines near pos=0, legend overprinting itself
    illegibly. Pure pseudo-continuous artifact of a discrete counter.
14. **transition_matrix** — Did not render: explicit card "N/A for bool, enum, int
    state: could not build state set 'st.s3'." Honest failure.

(occupancy_heatmap and orbit_scatter produced no file.)

## Verdict

- **Keepers (3):** `time_series` (the honest, complete run, including the post-EOF
  leak), `timing_diagram` (same, cleaner framing), `morse_graph` (the reachable
  structure + the under-constrained terminal as an SCC). `reachability_tree` is a
  worthy fourth.
- **Drop:** every continuous-flow view — `phase_portrait`, `nullcline_field`,
  `basin_map`, `cobweb`, `scatter_matrix` — each treats a bounded discrete
  counter+stack as a continuous real flow, fabricating ±thousands-scale axes the
  program never visits. `fixedpoint_map` is honest but near-empty; `transition_matrix`
  bailed (correctly) on the int+enum state with an N/A card.
- **Notable finding:** the `s0`/`s1` lanes in time_series are the only place the
  intentional bug-shaped feature surfaces — after `done` latches, the slots keep
  getting rewritten to stale Open kinds, which morse_graph then collapses into a
  spurious "cycle ×3" SCC. That is the documented post-EOF under-constraint made
  visible: the slots are free once they fall below `depth`. The graph-based views
  (morse_graph, reachability_tree) correctly register this as an absorbing
  recurrence, while `fixedpoint_map` reports "no fixed points found" — a real
  disagreement, and only the graph views caught it. A reader who saw only the
  continuous-flow plots would learn none of this.
