# lru — visualization review

## What the program does

`lru` is an LRU (least-recently-used) cache simulator run as an FSM. The carried
state is a `Cache` record: a cursor `pos` into a fixed access stream, three slot
keys (`k0,k1,k2`, with `-1` = empty), three recency timestamps (`r0,r1,r2`), two
accumulators (`hit_count`, `miss_count`), and a `done` flag. The transition reads
`input[pos]` (the fixed stream `⟨1,2,3,1,4,1,-1⟩`), classifies the access as a
HIT (key resident → bump that slot's recency) or a MISS (key absent → write it
into the LRU/empty victim slot), advances `pos`, and tallies stats. It is a
**finite, terminating run**: at pos=6 the sentinel `-1` sets `done=true`, after
which the transition is a fixed point (every counter freezes — except, see the
notable finding below). There is no cycle and no nontrivial attractor; the system
counts up and halts. Any viz that claims otherwise is fabricating.

## Ranked, best → worst

1. **timing_diagram** — The clearest single picture: all 10 state vars stacked,
   bool `done` on its own digital lane, ints as analog. You read the whole story
   directly — three cold misses fill `k0/k1/k2` (rising off `-1`), `miss_count`
   steps 0→4 then flatlines, `k1` gets evicted-and-rewritten (the `2`→`4` jog),
   `pos` ramps to 7 and freezes, `done` flips at tick 7. Faithful, dense, honest
   about the small real value range.
2. **time_series** — Same content as the timing diagram, importance-ordered, and
   it exposes the **bug** (see notable finding): `hit_count` climbs *linearly and
   forever* past the halt while every other var is frozen. That divergence is the
   most informative single signal in the whole gallery.
3. **reachability_tree** — A clean spine of 8 real reachable states from the seed
   down to `(True, 2,1,4,3,4, 7,6,5,3)`, then a fan-out of orange `done=true`
   children. The spine is exactly the true run; the fan is the bug leaking into
   the BFS (post-halt successors that should be a single self-loop).
4. **transition_matrix** — A mostly-diagonal sampled transition matrix, blue
   (`done=false`) block in the upper-left, orange (`done=true`) lower-right. Reads
   as "near-deterministic march, then absorbing region" — correct in spirit,
   though it is over a sampled state set, not the 8 real states.
5. **state_graph** — The blue lower-left chain (the real run) is genuinely
   informative; the orange upper-right fan of self-looping `done=true` nodes is
   the post-halt artifact again. Half-truth: trust the blue spine, ignore the fan.
6. **parallel_coords** — Shows the per-axis value ranges and the false/true class
   split with reasonable fidelity, but the dense orange ribbon bundle is the
   over-sampled `done=true` set, so it over-weights states the real run never
   dwells in.
7. **chord_diagram** — Honest and minimal: two nodes `false`/`true`, a single
   one-way arc plus a `true` self-loop. Correctly says "monotone flip into an
   absorbing state," but it only knows about `done`, so it is low-information.
8. **scatter_matrix** — A 10×10 grid that is technically faithful (orange/blue =
   done) but unreadable at this size; the only legible cells confirm `hit_count`
   spreads while the others cluster. Confirmatory, not explanatory.
9. **orbit_scatter** — Plots 4 arbitrary numeric seeds (`k0` from −1500 to +2800,
   `miss_count` to 2700) that the real program never visits. The seeds are
   fabricated; only by accident does it land near sane axes.
10. **nullcline_field** — A `(k0, miss_count)` sign-region field over roughly
    ±3000 on both axes. The real `k0∈{-1,1}` and `miss_count∈{0..4}` — the entire
    plotted plane is fiction. Empty noise.
11. **cobweb** — A perfect `y=x` line of `st.k0(n+1)` vs `st.k0(n)` sampled over
    ±3300, with a red "orbit" seeded at 2000. `k0` is a cache *key*, not a 1-D
    map; cobwebbing it is a category error, and the 2000-seed is off in fabricated
    space.
12. **occupancy_heatmap** — Claims "where the system dwells": a vertical stripe at
    `k0=1` spanning `miss_count` from −3000 to +3000. The dwell point is actually
    one state (`k0=1, miss_count=4`); the stripe is the ±3000 sampler painting a
    dwell-region that does not exist.
13. **phase_portrait** — Worst. A faceted vector field over `k0` × `miss_count`
    on ±5000 axes with a red orbit loop. The chosen axes (a key and a counter)
    have no dynamical relationship, the plane is entirely outside the real range,
    and the red loop *invents a cycle* in a program that strictly counts up and
    halts. Pure fabrication.

## Verdict

**Keepers:** `timing_diagram` (the single best whole-program view),
`time_series` (same content, and it surfaces the hit_count bug most starkly),
and `reachability_tree` (shows the true 8-state spine).

**Drop for this program:** `phase_portrait`, `cobweb`, `occupancy_heatmap`,
`nullcline_field`, `orbit_scatter` — all five are the hardcoded ±3000/±5000
numeric-axis renderers painting structure (cycles, basins, dwell-stripes, vector
fields) over a plane the cache never enters. For a 10-Int/1-bool counting FSM
whose real values live in `{-1..8}`, they are not just uninformative, they are
actively misleading.

**Notable finding (a real under-constraining bug, not just a viz artifact):**
both `time_series` and `timing_diagram` show `hit_count` rising **linearly and
without bound after the stream ends** — at tick 60 it is ~55 while every other
state variable (keys, recencies, miss_count, pos) is frozen and `done=true`. The
transition guards key/recency/pos/miss writes on `¬is_first_tick` with `at_end`
folded in, but `st.hit_count = _st.hit_count + (is_hit ? 1 : 0)` keeps counting:
once `done`/`pos` are pinned at the sentinel index, `is_hit` evidently stays true
each post-halt tick and `hit_count` ticks up forever. The halt is supposed to be
"a tick changes nothing," but `hit_count` changing every tick means **the program
never reaches a fixed point** — it only stops because the runtime's effect loop
has nothing left to dispatch, not because the state stabilized. The
`reachability_tree` and `state_graph` fan-outs are the same bug viewed through
BFS: post-`done` successors that should collapse to one self-loop instead fan
into many distinct states because `hit_count` keeps incrementing. The static
`unsat_hit_is_not_a_miss` test guards the wrong axis; nothing pins hit_count after
`at_end`. The numeric vizzes' fabrications are noise, but this divergence is a
genuine correctness signal the time-domain plots caught.
