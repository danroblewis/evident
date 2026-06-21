# ledger — visualization review

## What the program does

`ledger` is a transaction-replay FSM: it streams a fixed 6-entry journal of
`(account, amount, Deposit/Withdraw)` records through a 3-account bank. Its
carried state is a record `Ledger { pos, b0, b1, b2, ok, done }` — a cursor
`pos`, three Int balances, a latched-invariant bool `ok` (never-overdrawn), and
a `done` flag. The transition is **deterministic with a single seed**: from the
all-zero start it advances `pos` one per tick, applies one journal entry
(deposit credits, covered withdraw debits, overdraft is rejected and leaves the
balance unchanged), and latches `ok=false` forever on the first overdraft. The
actual run is a **single linear orbit of 7 reachable states** ending at
`(pos=6, b0=0, b1=50, b2=20, ok=false, done=true)`; the last journal entry (a 70
withdraw from b0 holding only 20) is the overdraft that flips `ok`. All real
values live in a tiny range: `pos ∈ 0..6`, balances `0..100`.

## Ranked, best → worst

1. **time_series** — The honest portrait of this machine: six stacked lanes show
   `pos` ramping 0→6, `b0` doing 0→100→70→0, `b1`/`b2` stepping up and holding,
   `ok` dropping true→false exactly at tick 5, `done` rising at tick 6. Reads the
   whole behavior in one glance, including the overdraft latch.
2. **morse_graph** — A clean 7-node chain with each node labeled by its actual
   balances; the fill flips orange→blue at `pos=5` (the overdraft) and the
   terminal node is ringed green at `pos=6/done`. Exact, readable, faithful.
3. **timing_diagram** — Same content as time_series over 40 ticks, making the
   "everything is frozen after tick 6" fixed-point obvious; bool lanes (`ok`,
   `done`) rendered as digital traces is the right call for a latch machine.
4. **reachability_tree** — The literal 7-state spine with full state tuples on
   each node, root ringed green and the absorbing `done` node ringed red. Slightly
   redundant with morse_graph but the explicit tuples are nice.
5. **parallel_coords** — All 7 reachable states across the six axes, colored by
   `ok`; you can trace the one blue (ok=false) strand and confirm balances stay
   bounded. Compact and exact, if a touch busy.
6. **state_graph** — Correct 7-state/7-edge linear chain, but the node labels
   overlap into illegible mush and the legend swatch sits on top of a node. Right
   data, poor layout.
7. **cobweb** — Mechanically correct staircase for `pos_{n+1}=pos_n+1`, but `pos`
   is a trivial counter so the cobweb conveys nothing the time series didn't; the
   negative-`pos` samples below 0 are off the reachable set.
8. **transition_matrix** — A large sampled state-grid heatmap with scattered
   diagonal blips; the real machine has 7 states and one path, so this 60×60
   matrix is mostly fabricated off-trace sampling and the labels are unreadable.
9. **chord_diagram** — Collapses everything to the `ok` bool: two nodes
   (true→false) with a single arc. Technically true (ok does go true→false once)
   but throws away all balance/cursor structure — near-zero information.
10. **fixedpoint_map** — Claims "1 fixed point" at `(pos=6, b2=20)`, which is
    real, but the faceting and the stray reachable points around it add little;
    the morse/reachability views already show the terminal state better.
11. **orbit_scatter** — **Fabricated.** Axes span `pos ∈ -1500..2800`,
    `b2 ∈ 0..2700` — ranges the program never enters — with invented "seeds" and
    "attractors" scattered across that void. The real orbit is 7 points in `0..100`.
12. **scatter_matrix** — **Fabricated.** "588 sampled states" with axes to
    ±3000 / 20000; the marginals and the lone straight `b0` ridge are artifacts of
    sampling Int axes far outside the reachable `0..100` box.
13. **basin_map** — **Fabricated.** A uniform grid of identical blue ticks over
    `b2 ∈ -2..22`, `pos ∈ -1..7`, reporting "1 basin / 1 attractor" — it has just
    re-discovered that a deterministic machine has one attractor, dressed as a
    basin map. No real basin structure exists.
14. **occupancy_heatmap** — **Fabricated.** Axes `pos, b2 ∈ ±3000`; the "where
    the system dwells" claim is a regular lattice of bright cells across a 6000-wide
    void the system never visits. The actual dwell set is one frozen point.
15. **nullcline_field** — **Worst.** Axes ±3500, a dense black dot-grid with a
    pink background and four mutually-overlapping illegible legend entries
    (`dpos↑db2↑…`). Sign-region/nullcline analysis is meaningless for a
    finite-journal counter; this is pure noise.

## Verdict

**Keepers (2–3):** `time_series` (the single most informative+representative
diagram — it shows the cursor ramp, the b0 deposit/withdraw arc, and the `ok`
latch flipping at tick 5, all at once), `morse_graph` (exact labeled 7-state
chain with the ok-flip and terminal node), and `timing_diagram` (makes the
post-tick-6 fixed point unmistakable).

**Drop:** the entire numeric-sampling family — `nullcline_field`,
`occupancy_heatmap`, `basin_map`, `scatter_matrix`, `orbit_scatter`,
`fixedpoint_map`. Every one samples the Int axes over a hardcoded ±3000-ish
window and invents lattices, basins, and "attractors" in a region this
finite-journal machine never enters. `chord_diagram` and `transition_matrix`
are also droppable here: chord throws away everything but one bool; the
transition matrix is an unreadable off-trace sampling grid.

**Notable finding:** the known numeric-renderer range bug is florid on `ledger`.
The program is a deterministic 6-step replay with all values in `0..100` and a
single 7-state orbit, yet six diagrams paint structure across ±3000–20000 axes.
`basin_map` reporting "1 basins" and `occupancy_heatmap` showing a regular bright
lattice are the clearest fabrications — they manufacture spatial structure for a
machine whose entire phase space is seven points clustered near the origin. The
exact-trace views (time_series, morse_graph, timing_diagram, reachability_tree)
are the only ones that tell the truth about this program.
