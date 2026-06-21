# grep — visualization review

## What the program does

`grep.ev` is a streaming fixed-substring match counter — a terminating
utility, not a dynamical system. State is `GrepState(line_no, match_count,
matched, done)`. Each tick consumes the next input line (4 char-codes
selected by `_state.line_no` from a complete 4-row lookup), runs a real Z3
substring scan for pattern "ab" (`∃ start : line[start..]=pat`), then:
`line_no += 1`, `matched := (cur_matched ∧ n < 4)`, `match_count` steps up
on a match else holds, `done := line_no ≥ 4`. Over the canonical stream the
trajectory is exactly: `(0,0,F)→(1,1,T)→(2,2,T)→(3,2,F)→(4,2,F,done)`. So
the honest picture is a short, monotone, NON-cyclic chain: line_no climbs
forever, match_count saturates at 2, `matched` is a transient flag true on
exactly ticks 1–2, `done` latches at tick 4. There is no attractor, no
cycle, no fixed point — `line_no` is unbounded.

## Diagrams ranked, best → worst

1. **reachability_tree** — BEST. A vertical 9-node chain
   `(False,0,0,False)→(False,1,1,True)→(False,2,2,True)→(False,3,2,False)→…`
   reading the exact trajectory off the labels: line_no climbing, match_count
   stopping at 2, the orange `matched=True` nodes landing precisely on ticks
   1–2 (the two matching lines), blue thereafter. Faithful and legible. Keep.
2. **timing_diagram** — Excellent. Four stacked lanes: `line_no` ramping
   linearly, `matched` a clean 1-tick-wide pulse over ticks ~1–2, `match_count`
   the 0→1→2 staircase plateauing at 2, `done` latching high at tick 4. This is
   the program's behavior at a glance. Keep.
3. **time_series** — Same four traces over 61 ticks; importance-ordered. The
   `matched` pulse and the `match_count` plateau and `done` step are all true.
   Slightly redundant with timing_diagram but equally faithful.
4. **fixedpoint_map** (done=false panel) — Surprisingly good: three dots at
   (0,0)F, (1,1)T, (2,2)T — exactly the early trajectory and the two matches,
   correctly colored. Title honestly says "no fixed points / short cycles
   found." The done=true panel is a degenerate flat `match_count=2` line out
   to 5000, padding.
5. **cobweb** — The map `line_no(n+1)=f(n)` is the diagonal y=x+1, which IS the
   +1 increment, and that's correctly drawn as a tight diagonal. But it samples
   seeds from −3000…+3000, a state range the program never occupies, so it's a
   correct mechanism shown over a fictional domain. Half-useful.
6. **parallel_coords** — 4 axes (line_no, matched, match_count, done) with the
   orange `matched=true` ribbon crossing to `match_count=2` and `done=true`.
   Readable, captures the variable coupling, but the `line_no` axis to 399 is
   synthetic fan-out.
7. **chord_diagram** — Minimal but honest: two nodes `matched=false` /
   `matched=true` with a single false→true arc. Tells you `matched` flips once;
   not much more.
8. **state_graph** — Nearly degenerate: a single long horizontal row of ~300
   blue nodes at one match_count, with one stray node dropping down. "300
   states" is sampled fiction; the real reachable set is ~5 states.
9. **transition_matrix** — Sparse diagonal-ish smear of yellow cells on a purple
   grid, labels unreadable at this size. Technically shows "each state → next"
   but conveys nothing usable.
10. **scatter_matrix** — A 4×4 grid over a −3000…+3000 sampled cloud. The
    histograms and pairwise panels are dominated by synthetic seeds; the real
    structure (a 5-point chain) is invisible. Noise.
11. **phase_portrait** — Degenerate. A vector field of tiny arrows over
    `line_no × match_count ∈ ±4000`, faceted by done. Imposes a continuous
    flow-field reading on a discrete terminating counter. Misleading.
12. **orbit_scatter** — Multiple synthetic seeds at ±3000 fanning into smears
    labeled "attractor." There is no attractor; the program counts and halts.
13. **occupancy_heatmap** — "where the system dwells": a regular ±3000 polka-dot
    lattice from quantized synthetic seeds. The system dwells nowhere near
    these cells. Pure artifact.
14. **nullcline_field** — Unreadable: a dense black dot-grid over ±3500 with
    overlapping legend text, "sign-regions of (dline_no, dmatch_count)." A
    continuous-ODE diagram forced onto an integer counter. Worst kind of fit.
15. **basin_map** — Invents **6 attractor basins** with bogus centroids
    (line_no≈794, 4112, 4824, −2698…) each tagged "(cycle)". The program has
    ZERO cycles and ZERO basins — this is fabricated structure.
16. **morse_graph** — WORST. A "condensation" with a red box labeled
    `cycle ×2` and 153 edges fanning between fictional grid cells at
    coordinates like (3200,3200). It asserts a recurrent cycle in a program
    that strictly increments and terminates. Actively wrong.

## Verdict

- **Keepers:** `reachability_tree` (the exact trajectory, labeled),
  `timing_diagram` (behavior of all four state vars at a glance), and
  `time_series` as the third. `fixedpoint_map`'s left panel is a nice bonus.
- **Drop:** `morse_graph`, `basin_map`, `occupancy_heatmap`, `nullcline_field`,
  `orbit_scatter`, `phase_portrait`, `scatter_matrix` — every one of these
  samples a ±3000/±4000 synthetic state space the program never visits and
  imposes dynamical-systems vocabulary (cycles, basins, attractors, nullclines,
  flow fields) on a 4-tick terminating counter.
- **Notable finding (a picture revealing a generator bug, not a program bug):**
  `morse_graph` and `basin_map` both INVENT cycles/attractors — morse_graph
  literally draws a `cycle ×2` box, basin_map lists "6 basins … (cycle)". The
  program has no cycle at all (`line_no` strictly increments and is unbounded).
  This is the seed-sampling/quantization pipeline manufacturing recurrent
  structure out of an unbounded monotone counter: it wraps/quantizes large
  synthetic `line_no` values into a finite lattice, and the wrap reads back as
  a cycle. Any viz that seeds from a wide synthetic numeric range
  (±3000) is structurally unfaithful for this class of unbounded-counter
  program — the real reachable set is ~5 states, all of which the
  reachability_tree shows correctly.
