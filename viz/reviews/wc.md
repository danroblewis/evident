# wc — visualization review

## What the program does

`wc.ev` is a `wc`-style multi-counter. Its carried state is `WcState =
{cursor, lines, words, chars, in_word}`. A cursor walks a fixed 10-codepoint
stream `"hi  yo\nok\n"` one char per tick: every char bumps `chars`; a newline
(10) bumps `lines`; `in_word` tracks word-ness; `words` increments only on the
rising edge (out-of-word → in-word). When `cursor ≥ 10` the tally **freezes** at
`(cursor=10, lines=2, words=3, chars=10, in_word=false)` — a single absorbing
fixed point and halt. There is exactly ONE trajectory: a linear 11-state chain
from `(0,0,0,0,F)` to the frozen terminal. No branching, no cycle, no basin.
The state values are tiny (0–10). This is the kind of program the numeric-axis
samplers can most badly misrepresent.

## Ranked best → worst

1. **morse_graph** — The faithful gold standard: the exact 11-state linear chain
   with real `chars/words/lines/in_word` labels, the green absorbing terminal at
   `chars=10 [words=3 lines=2]`, in_word coloring per node. This *is* the program.
2. **time_series** — Four stacked tracks over 12 ticks show `chars` ramping
   0→10, `words` stepping 0→3, `lines` 0→2, and the `in_word` square wave toggling
   exactly at the whitespace/newline boundaries. Reads like a textbook scan trace.
3. **timing_diagram** — Same trace over 40 ticks, making the freeze undeniable:
   all four signals flatline after tick 10. The clearest "it halts" picture.
4. **state_graph** — The real reachable chain (11 states, 11 edges) with the
   terminal self-loop ring and in_word coloring; only flaw is overlapping node
   labels crowding each other.
5. **fixedpoint_map** — Scans the *reachable* set and correctly reports exactly
   **1 fixed point** at `(chars=10, words=3)`, starred. Honest because it uses the
   reachable scan, not the ±3000 grid.
6. **reachability_tree** — Correct linear BFS spine (9 capped nodes, depth 8) with
   right in_word colors; just a truncated, less-labeled version of morse_graph.
7. **cobweb** — Honest staircase: `chars(n+1)=chars(n)+1` climbing above `y=x`,
   showing chars never reaches a fixed point along this axis (it's the cursor that
   halts, not chars). Faithful but holds the other counters fixed, so partial.
8. **parallel_coords** — 11 reachable states across chars/words/lines/in_word axes;
   technically correct but the crossing lines for an 11-point linear walk add little.
9. **chord_diagram** — Reduces everything to the `in_word` bool: false↔true with a
   thick true→false arc. Captures the word-boundary toggle but throws away the
   counters that are the whole point of `wc`.
10. **scatter_matrix** — 234 "sampled states" spread over chars/words ∈ ±3000.
    The real data lives in a 0–10 dot in the corner; the grid is fabricated.
11. **orbit_scatter** — Four seeds at chars≈−1500/0/400/2800, words≈0/1500/2700 —
    none of which the program can ever occupy. Invented entirely by the sampler.
12. **basin_map** — Claims **2 basins**, including "basin 1: chars≈4005, r≈3
    (cycle)". The program has NO cycle and never reaches chars=4005. Fabricated.
13. **occupancy_heatmap** — Hot cells scattered across a ±3000 chars/words field
    titled "where the system dwells." It dwells at `(10,3)`; everything shown is noise.
14. **nullcline_field** — A ±3000 diagonal-hatched sign-region field. Meaningless
    for a program whose Δ is "+1 char per tick until index 10, then 0."

(transition_matrix did not render — file absent from `viz/gallery/`.)

## Verdict

**Keepers (3):** `morse_graph`, `time_series`, `timing_diagram`. Together they
nail the structure (linear chain → absorbing terminal), the per-tick counting
semantics, and the halt. `state_graph` and `fixedpoint_map` are solid backups.

**Drop:** `basin_map`, `occupancy_heatmap`, `orbit_scatter`, `scatter_matrix`,
`nullcline_field` — all five are the hardcoded ±3000 Int-axis samplers
hallucinating attractors, basins, "dwell" regions, and sign fields over a phase
space the program enters in a 0–10 box and never leaves. `phase_portrait` is the
flagship offender (below).

**Notable finding:** the numeric-renderer fabrication bug is on full display
here. `phase_portrait` tiles ~200 red "fixed point" stars across a ±4000
chars/words grid — a program that has exactly ONE fixed point and counts to 10.
`basin_map` goes further and *labels* a non-existent cycle: "basin 1: chars≈4005
… r≈3 (cycle)". This program can never branch, cycle, or exceed chars=10, so
every diagram that sampled the wide Int grid invented its entire content. The
renderers that scanned the *reachable* set instead (morse_graph, state_graph,
fixedpoint_map, reachability_tree) are all correct — a clean confirmation that
the fix is "scan reachable, not a hardcoded box."
