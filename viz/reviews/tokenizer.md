# tokenizer — visualization review

## What the program does

`tokenizer` is a lexer run as an FSM. It walks a *fixed* 9-element class stream
`⟨Digit, Digit, Space, Letter, Letter, Symbol, Space, Letter, CEnd⟩`
("12 ab+ c") one character per tick. The carried state (`Lexer`) is five fields:
`pos` (cursor, 0→9), `mode` (an enum over `InNumber/InWord/InSymbol/InSpace`),
`tok_len` (length of the current run), `tokens_emitted` (completed-token count),
and `done` (latches once `CEnd` is consumed). It is a strictly finite, fully
deterministic, **terminating** trajectory: `pos` ramps 0→9, `mode` cycles
through the four lexer states the input visits, `tok_len` sawtooths up-then-reset
per run, `tokens_emitted` steps 0→4, and at `pos=9` `done` flips true and the
state freezes (the only fixed point). There are no cycles, no basins, no
multi-trajectory structure — it's a single line that runs once and stops.

## Ranked: best → worst for THIS program

1. **time_series** — The definitive read of this program: all five fields on one
   tick axis, you see `pos` ramp, the `tok_len` sawtooth (1→2→0, 1→2→0, 1, 0),
   `tokens_emitted` stepping 0→1→2→3→4, `done` latching at tick 9, and `mode`
   cycling InSpace→InNumber→InSpace→InWord→InSymbol→InSpace→InWord→InSpace.
   Every claim in the source is legible at a glance. Nothing fabricated.
2. **timing_diagram** — Same data, lane-typed (bool digital / enum lane /
   int analog), and it extends the run to 40 ticks so you SEE the halt: after
   tick 9 every field flat-lines. Best demonstration that this FSM terminates.
3. **morse_graph** — A clean 10-node vertical chain with full state labels per
   node and the terminal `pos=9 done` node ringed green. This is the honest
   "what states does it actually reach" answer: a path, not a graph. Faithful.
4. **reachability_tree** — A literal single spine of 9 nodes, root ringed green.
   Correctly shows the trajectory is a line with no branching. Slightly redundant
   with morse_graph; loses points only for the alarming "unbounded — capped
   sample" caption that doesn't apply (the reachable set is 10 states, period).
5. **state_graph** — Same chain laid out 2D over (pos, tok_len) with the absorbing
   `done` self-loop drawn as an orange ring. Readable, faithful; the overlapping
   bottom-row labels are a minor legibility ding.
6. **fixedpoint_map** — Correctly finds exactly **1** fixed point (`pos=9,
   tok_len=0, done=true, InSpace`), faceted done=false/true with the reachable
   pre-halt states colored by mode. Honest and matches the source's single
   absorbing state. The done=true panel is nearly empty (one star) — accurate
   but sparse.
7. **parallel_coords** — All 10 reachable states as polylines across the 5 axes,
   colored by done. You can trace the orange (done=true) terminal line and the
   blue body. Dense but truthful; not the first thing you'd reach for to
   understand the lexer.
8. **cobweb** — On `state.pos` it's just `pos → pos+1`, so the orbit is a
   staircase hugging y=x and the done=true panel is the y=x identity. Technically
   correct (pos is a counter) but conveys nothing the time_series didn't, and
   picking `pos` as the scalar is the least interesting field.
9. **chord_diagram** — Two nodes (done=false → done=true) with a single arc and a
   self-loop on true. Faithful (done flips once and latches) but trivially small;
   the four-color mode legend is mostly unused.
10. **transition_matrix** — A 36×36 sampled-state matrix dominated by a diagonal
    of self-loops (absorbing samples) plus a scatter of off-diagonal points. The
    real transition is a 10-state chain; this buries that under sampling noise and
    is unreadable at this resolution.
11. **basin_map** — **FABRICATED.** Claims "2 basins," tiles a `pos`×`tok_len`
    grid, and asserts one basin with attractor `pos≈10, tok_len≈2`. The program
    has exactly one absorbing state (`tok_len=0`, not 2) and no basin structure —
    it's a single finite trajectory. The scattered red cells at `pos=9` are pure
    sampling artifact.
12. **nullcline_field** — **FABRICATED.** A `pos`×`tok_len` quiver over roughly
    ±3500 on both axes with "nullclines," treating a discrete cursor-counter lexer
    as a continuous 2D vector field. The program never leaves `pos∈[0,9],
    tok_len∈[0,2]`; this invents a flow over a space it never enters. Meaningless.
13. **scatter_matrix** — **FABRICATED + degenerate.** Title says "1788 sampled
    states"; `pos` and `tok_len` axes span ±3000. The real values live in
    `pos∈[0,9]`, `tok_len∈[0,2]`. The diagonal smears and off-axis point-clouds
    are entirely the ±3000 hardcoded sampler exploring states the program cannot
    reach. Actively misleading about the program's range.
14. **phase_portrait** — **WORST.** Quiver of (pos, tok_len) over ±4000, faceted by
    done. The done=true panel is a wall of red stars (every sampled cell flagged
    "absorbing") and the done=false panel is a uniform downward flow field with
    spurious orbit segments. The only real motion is `pos: 0→9, tok_len: 0..2` —
    a tiny corner — yet the picture is dominated by thousands of fabricated cells.
    The named demo axes (`tokens_emitted, mode`) aren't even what's plotted.

## Missing renders

- **occupancy_heatmap** — not generated (no `occupancy_heatmap__tokenizer.png`).
- **orbit_scatter** — not generated (no `orbit_scatter__tokenizer.png`).

## Verdict

**Keepers:** `time_series`, `timing_diagram`, `morse_graph`. Between them you get
the full per-field trajectory, visible proof of termination, and the honest
reachable-state chain. `fixedpoint_map` is a worthy fourth — it nails the single
absorbing state.

**Drop (for this program):** `phase_portrait`, `scatter_matrix`, `nullcline_field`,
and `basin_map` — all four are dominated by the ±3000/±4000 hardcoded numeric
sampling range and **fabricate structure** (basins, attractors, vector flow,
point-clouds) over a state space the lexer physically never enters. `cobweb`,
`chord_diagram`, and `transition_matrix` aren't *wrong* but are trivial or noisy
for a finite linear walk.

**Notable finding:** This program is the cleanest possible illustration of the
known numeric-sampler bug. Its true reachable set is **10 states** in a box of
`pos∈[0,9]`, `tok_len∈[0,2]`, `tokens_emitted∈[0,4]`. Four of the numeric
renderers instead sample over ±3000–4000 and report invented basins
(`basin_map`: "2 basins, attractor tok_len≈2" — the real fixed point has
tok_len=0), invented flow (`nullcline_field`), and 1788 phantom states
(`scatter_matrix`). A reader trusting `basin_map` or `phase_portrait` would
conclude this lexer has attractor/basin dynamics when it simply counts to 4 and
halts. The discrete/graph renderers (morse, reachability, time_series, timing)
that work from the *actual* reachable set get it exactly right — strong evidence
the fix is to bound the numeric samplers to the reachable set rather than a
hardcoded range.
