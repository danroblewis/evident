# life — visualization review

## What the program does

`life` runs Conway's Game of Life on a fixed 4×4 grid as a difference equation.
The carried state is the grid itself: 16 Bool cells (`c00..c33`, row-major), a
generation counter `gen ∈ Int`, and a derived live-cell census `pop ∈ Int`
carried purely for the phase portrait. Each tick recomputes every cell from its
clipped 8-neighbour count under B3/S23 (no wraparound). The first tick seeds a
horizontal blinker in row 1; thereafter the blinker oscillates with period 2
(horizontal ↔ vertical), so the *only* real trajectory this program ever
exhibits is: `gen` increments by 1 forever, `pop` pins at 3, and the cell bits
flip on a period-2 cycle. There is exactly one orbit. Anything a diagram shows
beyond "a counter climbing while a few bits toggle 2-periodically at constant
population" is fabricated.

## Ranked: best → worst (for THIS program)

1. **timing_diagram** — The single most faithful picture: `gen` ramps linearly,
   `pop` is a dead-flat line at 3, and `c01` is a clean period-2 square wave.
   That is literally and completely what `life` does. Three lanes, no noise.
2. **time_series** — Same three signals, same truth (gen ramp, pop≡3, c01
   period-2 square wave), and it correctly labels `pop` as a near-constant
   quant. Slightly busier title than timing_diagram but equally honest.
3. **reachability_tree** — An honest linear chain of 9 distinct states whose
   labels show the blinker actually flipping (`...True,True,True...` ↔
   `...,True,...,True,...`) while gen counts 0..8 and pop stays 3. A real BFS of
   the real orbit; correctly shows no branching.
4. **chord_diagram** — Reduces to `state.c01` as a bool and shows a single
   bidirectional false↔true arc. Minimal but exactly right: the toggling bit
   *is* a 2-state flip-flop.
5. **parallel_coords** — Reads correctly once decoded: gen spans 0..399, c01
   splits cleanly into the two classes, and both classes land at pop 3 (only the
   axis padding to 2/4 hints otherwise). Faithful but the gen-axis fan is visual
   clutter that adds nothing.
6. **morse_graph** — Thumbnail-tiny and hard to read, but the structure (a top
   recurrent band over a quantized lattice condensing down) is at least not
   inventing a phantom attractor; it's a degraded view of the real flow.
7. **state_graph** — A single horizontal line of 300 alternating blue/orange
   nodes with an orange "terminal" cap. The alternation is real (c01 toggling),
   but it plots them along the fabricated `state.gen` axis out to absurd extents
   and labels a node "fixed point" that isn't one — half-true.
8. **cobweb** — A pure `y=x` diagonal because `gen_{n+1}=gen_n+1` is an affine
   counter; technically correct (a counter has no interesting cobweb) but it
   draws the line over ±3000, a range the program never visits. Uninformative by
   construction.
9. **scatter_matrix** — Shows c01 is bimodal (T/F) and pop is a single spike at
   3 (both true), but every panel involving `gen` smears across ±3000 of
   fabricated values, and the gen histogram invents a multi-modal distribution
   out of sampling artifacts. More misleading than useful.
10. **nullcline_field** — Sign-region vector field over gen∈[-3500,3500],
    pop∈[-3500,3500]. The dgen↑ everywhere is the one true fact (gen always
    increases); everything else (the pop↑/pop↓ split around 0, the red nullcline
    at pop≈0) is pure artifact of sampling a Bool-derived census as if it were a
    continuous −3500..3500 quantity.
11. **occupancy_heatmap** — One faint red cell at (gen≈0, pop≈3) is the truth;
    the rest is a black field with scattered specks strewn across ±3000 of gen
    that the system never occupies. The hardcoded range drowns the one real dwell
    point.
12. **orbit_scatter** — Claims "multiple seeds → attractor" and plots seeds at
    pop 1500 and 2700 — values `pop` can never take (max is 16, real is 3). The
    flat band at pop≈0 is the only honest part; the rest is fabricated seeds in
    impossible state space.
13. **basin_map** — The worst offender: announces "2 basins" and an "attractor"
    with `pop≈0`, splitting a ±3000 gen grid into two colored basins. `life`
    has ZERO basins and ZERO numeric attractor — it's a period-2 cycle at
    constant pop=3. This diagram invents a basin structure that does not exist.
14. **phase_portrait** — The headline viz, and it's almost entirely fiction: a
    vector field over gen,pop ∈ ±4000 with phantom fixed-point rings, arrows
    converging on a non-existent attractor near the origin, and step-magnitude
    coloring of states the program never enters. The real portrait is a single
    point marching right at pop=3; this shows a fabricated 2-D flow.

## Verdict

**Keepers (2–3):** `timing_diagram`, `time_series`, `reachability_tree`. These
three tell the whole truth about `life` — a generation counter climbing, a
constant population of 3, and a period-2 bit oscillation — with zero invention.
`chord_diagram` is a fine fourth as a one-glance "the toggling cell is a
flip-flop."

**Drop:** Every numeric-field renderer here is actively misleading on this
program — `basin_map` (invents 2 basins + an attractor), `phase_portrait`
(phantom flow + fixed-point rings), `orbit_scatter` (seeds at impossible
pop=1500/2700), `nullcline_field`, `occupancy_heatmap`, `cobweb`, and
`scatter_matrix`. All of them sample `gen`/`pop` over a hardcoded ~±3000–4000
range when the program's true footprint is `gen∈{0..60}, pop≡3`.

**Notable finding:** This program is a textbook reproduction of the known
hardcoded-range bug. `life`'s `pop` is a census that can only range 0..16 and in
practice is *constant at 3*; `gen` is a monotone counter. Yet `basin_map` reports
"2 basins on a 790-seed grid" with an attractor at `pop≈0`, `orbit_scatter`
draws seeds at `pop`=1500 and 2700 (numerically impossible — pop maxes at 16),
and `phase_portrait`/`nullcline_field` render full 2-D convergent flows with
fixed-point rings. There is no attractor, no basin, and no 2-D flow — there is
one period-2 orbit at fixed population. The numeric renderers fabricate all of it
by sampling axes the system never visits.
