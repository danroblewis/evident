# cut — visualization review

## What the program does

`cut` is a streaming field-extractor modelled as an FSM, emulating `cut -d: -f2`
over four pre-split lines. Carried state is `CutState = {line_no ∈ Int, field ∈
String, field_count ∈ Int}`. The transition is a monotone cursor advance —
`line_no := min(_line_no + 1, 5)` — followed by a COMPLETE ternary lookup keyed on
the new `line_no` that selects `(field, field_count)`: line 1 → ("30", 3), line 2 →
("25", 3), line 3 → ("40", 4), line 4 → ("", 1), and line 5 → ("", 0) the END
sentinel. There is **no cycle, no search, no branching**: it is a finite linear
walk of exactly 6 states (`line_no` 0→5) terminating in a single absorbing
fixpoint at `(line_no=5, field="", field_count=0)`. Behaviorally it is a tape
reader that runs off the end and parks. Any faithful picture should show a 6-node
chain ending in a self-loop.

## Ranked best → worst

1. **morse_graph** — The cleanest, most honest rendering: a vertical 6-node chain
   `line_no=0 → 1 → 2 → 3 → 4 → 5`, each node labeled with its full
   `[field_count, field]` payload, terminal node ringed. This IS the program — the
   tape and its contents, in order. Top pick.
2. **state_graph** — Same 6-state chain laid out in `(line_no, field_count)` space
   with the END node carrying a visible self-loop (the fixpoint) and colored by
   `field`. Faithful and adds the self-loop the morse graph omits; the (val,count,line)
   node labels are readable. A very close second.
3. **timing_diagram** — Three stacked lanes over 40 ticks show exactly the real
   dynamics: `line_no` ramps 0→5 then flatlines, `field_count` spikes 3,3,4,1 then
   drops to 0, and the `field` lane displays the literal strings "30"/"25"/"40"
   before going empty. Reads like a logic-analyzer trace of the extraction. Excellent.
4. **time_series** — Same content as the timing diagram (line_no ramp, field_count
   bump, field flat), 7 ticks. Faithful and immediately legible; slightly redundant
   with timing_diagram, and the categorical `field` lane is a flat red line carrying
   no info.
5. **reachability_tree** — A correct single-file chain of the 6 reachable states,
   root and absorbing node ringed. Accurate but it's a degenerate "tree" (no
   branching), so it's just the morse/state graph drawn as a column with a lot of
   whitespace.
6. **chord_diagram** — Over `field` values it shows the cycle "" → 30 → 25 → 40 → ""
   as arcs around a diamond. The arrows are real transitions, but the layout
   *implies* a recurrent cycle that does not exist (the run never revisits 30 after
   reaching ""), so it slightly misleads on the dynamics.
7. **parallel_coords** — Three axes (line_no, field_count, field) with 6 colored
   lines. You can trace each state's three coordinates, but with only 6 states and
   no clustering it conveys less than the time series and takes more effort to read.
8. **transition_matrix** — A near-empty grid: 5 bright diagonal cells on a sea of
   purple. Technically correct (each state goes to exactly one next state) but the
   from/to axis labels are illegibly tiny and 99% of the panel is empty — a
   permutation matrix this sparse is better shown as the chain.
9. **fixedpoint_map** — Correctly finds the one fixed point and plants a star at
   `(5, 0)`. True but trivial: a single dot on an empty axis tells you "there's one
   fixpoint" and nothing about how you get there.
10. **basin_map** — A 56-seed grid all in one color with the attractor star at
    `(5,0)`. Faithful (everything flows to the single absorbing state) but
    information-free — one basin, one color, no structure to see.
11. **cobweb** — Staircase of `line_no(n) → line_no(n+1)` climbing to 5 then flat on
    y=x. The orbit portion (red) is genuinely correct and readable, but the blue
    scatter scans line_no down to −2 and up to 7, plotting nonsense out-of-domain
    samples (the FSM never has a negative cursor). Misleadingly suggests a continuous
    map where there is a discrete tape.
12. **scatter_matrix** — 586 sampled states over a ±3000 range on all three axes.
    The real states occupy a tiny cluster near the origin; the rest is a uniform dot
    lattice of unreachable garbage. The `field` row/col is a flat line (one numeric
    value). Mostly noise; the real behavior is invisible in it.
13. **nullcline_field** — A sign-region quiver over ±3000 in (line_no, field_count)
    with colored half-planes. This treats a discrete table-lookup FSM as a smooth
    vector field; the "nullclines" are an artifact of the ternary arithmetic, not a
    meaningful structure. Thumbnail-tiny and unreadable to boot.
14. **phase_portrait** — A dense ±3000 numeric vector field. The cursor dynamics are
    a 1-step ramp confined to line_no∈[0,5]; rendering it as a continuous flow over a
    6000-unit square is a total forced fit — the actual trajectory is a few pixels
    near the origin, drowned in arrows that correspond to no reachable state. Worst.

## Verdict

**Keepers:** `morse_graph` (the chain + payload, the definitive picture),
`state_graph` (adds the fixpoint self-loop), and `timing_diagram` (the trace of the
three carried fields over time). Between them you see the tape, its contents, and
the run.

**Drop:** `phase_portrait`, `nullcline_field`, and `scatter_matrix` are forced
fits — they impose a continuous-dynamical-system frame on a finite discrete tape
reader and render the real behavior as a sub-pixel speck inside a ±3000 field of
unreachable samples. `basin_map` and `fixedpoint_map` are correct but trivially
single-valued.

**Notable finding:** the **cobweb** (and the wide-range scatter/phase plots)
expose that the generators scan `line_no` *outside its reachable domain* — into
negatives and beyond 5 — where the underlying ternary still evaluates. The program
itself clamps the cursor so this is harmless, but the viz reveals the model's
transition function is total over all of Int rather than guarded to the valid
cursor range; for an unclamped variant this same scan would be the picture that
catches an under-constrained cursor.
