# Visualization reviews — 24 sample programs critiqued

Each program in the corpus had all 16 visualization types generated, then an
independent critic agent read the program's source, *viewed* every diagram, and
ranked them by how informative and faithful they are to the program's actual
behavior. Per-program reviews are the `<program>.md` files in this directory; this
is the synthesis.

---

## The headline finding: the numeric renderers FABRICATE structure

The single most important result, flagged independently by nearly every reviewer:

**The numeric-axis renderers sample Int axes over a hardcoded ±3000–4000 box and
invent dynamics that do not exist.** Because they grid a guessed range (tuned for
the van der Pol oscillator's ±2000 scale) instead of the program's *reachable*
states, on a program that simply counts to 10 and halts they draw:

- `phase_portrait` — ~200 red "fixed point" stars carpeting a ±4000 plane (wc, histogram);
- `basin_map` — invented and *labeled* basins/cycles (`basin 1: chars≈4005 (cycle)`)
  that don't exist, sometimes reporting a **wrong attractor** that contradicts the
  exact renderers beside it (uniq);
- `morse_graph` (numeric fallback) — a "cycle ×2" SCC on a strictly-terminating counter (grep);
- `occupancy_heatmap`, `orbit_scatter`, `scatter_matrix`, `nullcline_field`, `cobweb`,
  `chord_diagram` — seed clouds, sign-fields, and continua over a ±3000 phase space
  the program never enters.

This is the **dangerous inverse of honest under-constraining**: not the program
being loose (which we *want* to see in a picture), but the *tool hallucinating
structure* — making a terminating program look like it has limit cycles. The
gallery critiquing itself is what surfaced it.

**The fix is the same everywhere:** numeric renderers must derive their sampling
window from `reachable()` / `trajectory()` — where the program actually lives —
and grid a fixed box only for genuinely unbounded continuous dynamics. Every
renderer that already scans the reachable set (`morse_graph`, `time_series`,
`reachability_tree`, `state_graph`, `fixedpoint_map`, `timing_diagram`) is
*exactly correct* on the same programs — clean confirmation of the fix.

---

## The view taxonomy (what to keep, what to drop)

| family | renderers | verdict |
|---|---|---|
| **Reachable-set / temporal** | `morse_graph`, `time_series`, `reachability_tree`, `timing_diagram`, `state_graph`, `fixedpoint_map` | **The keepers.** Faithful on every program shape because they trace the actual orbit / enumerate the real reachable set. |
| **Numeric-grid samplers** | `phase_portrait`, `basin_map`, `occupancy_heatmap`, `orbit_scatter`, `scatter_matrix`, `nullcline_field`, `cobweb`, `chord_diagram` | **Right only for genuinely continuous dynamics** (van der Pol). On bounded/discrete/terminating programs — i.e. *most of the corpus* — they fabricate structure and should be dropped or made to scan the reachable domain. |

**MVPs:** `morse_graph` (best for 9 programs), `time_series` (best for 8),
`reachability_tree` and `timing_diagram` round out the keepers. **For the one
continuous program (van der Pol), `fixedpoint_map` + the phase-plane family are the
keepers instead** — proving the samplers aren't universally bad, just mis-applied.

**Reviewers estimate ~⅓–½ of the 16 views add nothing for any given program** —
exactly the matching signal the contact sheet was built to surface.

---

## Per-program verdicts

| program | shape | best diagram | worst (the duds) | notable |
|---|---|---|---|---|
| **vanderpol** | continuous oscillator | `fixedpoint_map` | cobweb, basin_map, reachability_tree | the *only* program where the phase-plane family wins; fixedpoint_map names the unstable origin + the limit cycle |
| **dungeon** | discrete reachable graph | `morse_graph` | phase_portrait, cobweb, nullcline, scatter, time_series | trajectory views sample one degenerate empty-handed walk and contradict the rich branching the graph views reveal |
| **vending** | mixed limit cycle | `fixedpoint_map` | cobweb, morse_graph, basin_map | cobweb is an *active trap* — pins balance at 0 (the opposite of the truth) |
| **grep** | streaming counter | `reachability_tree` | morse_graph, basin_map, occupancy, phase_portrait | morse_graph + basin_map *fabricate cycles/basins* on a 5-state terminating counter |
| **cut** | finite tape reader | `morse_graph` | phase_portrait, nullcline, scatter | morse_graph literally *is* the tape and its fields in order |
| **brackets** | pushdown stack | `time_series` | nullcline, phase_portrait, basin_map, cobweb, fixedpoint_map | depth traces the stack exactly; the post-EOF under-constraint shows as drift |
| **find** | traversal frontier | `time_series` | nullcline, phase_portrait, basin_map, scatter | numeric views span ±3500 vs the real current ∈ −1..5 |
| **ps** | histogram / accumulator | `morse_graph` / `time_series` | basin_map, nullcline, phase_portrait, occupancy | basin_map invents 3 basins for a single-fixed-point program |
| **wc** | multi-counter | `morse_graph` | basin_map, occupancy, phase_portrait, orbit_scatter | phase_portrait tiles ~200 fixed-point stars over a ±4000 grid |
| **uniq** | run-length | `morse_graph` | phase_portrait, basin_map, occupancy, orbit_scatter | basin_map reports a *wrong* attractor contradicting morse_graph beside it |
| **csv_stats** | numeric aggregation | `time_series` | nullcline, cobweb, basin_map, chord | phase_portrait declares a ±1.3e6 plane stationary (sentinel-stretched) |
| **calc** | RPN eval stack | `morse_graph` | basin_map, scatter, orbit_scatter, occupancy | basin_map invents `pos≈4000 (cycle)` on a 6-token evaluator |
| **tokenizer** | lexer state machine | `time_series` | phase_portrait, scatter, nullcline, basin_map | the enum mode reads cleanly as a step lane |
| **ls** | cursor + accumulators | `morse_graph` | phase_portrait, nullcline, occupancy, orbit_scatter | a clean labeled chain of (cursor, total, largest) |
| **du** | traversal + aggregation | `time_series` | basin_map, nullcline, phase_portrait, cobweb | the recursive size accumulation reads as a staircase |
| **top** | running top-k | `morse_graph` | phase_portrait, occupancy, nullcline, scatter | the top-k slots updating are legible in the chain |
| **pstree** | tree from pointers | `time_series` | occupancy, nullcline, orbit_scatter, scatter | occupancy paints a periodic lattice when the program dwells at one point |
| **toposort** | partial order + ready-set | `morse_graph` | nullcline, phase_portrait, orbit_scatter, basin_map | nondeterministic ready-set choice shows as branching in the graph views |
| **scheduler** | priority queue + clock | `morse_graph` | nullcline, phase_portrait, transition_matrix, orbit_scatter | the job/clock progression is a clean chain |
| **ledger** | balance vector + invariant | `time_series` | nullcline, occupancy, basin_map, scatter | the latched overdraft flag + balances read per-tick |
| **lru** | cache + recency | `timing_diagram` | phase_portrait, cobweb, occupancy, nullcline | hit/miss + recency tracks are the natural view |
| **randomwalk** | stochastic walk | `reachability_tree` | nullcline, phase_portrait, cobweb, fixedpoint_map | the *nondeterministic fan* is honestly drawn by the reachable graph |
| **histogram** | binning | `time_series` | phase_portrait, occupancy, orbit_scatter, nullcline | phase_portrait carpets ±4000 with stars for values that live in [0,10] |
| **life** | 2-D cellular grid | `timing_diagram` | phase_portrait, basin_map, orbit_scatter, occupancy | the per-cell tracks show the generations; numeric views meaningless on a grid |

Per-program detail: open `viz/reviews/<program>.md`.

---

## Action items the reviews produced

1. **Fix the fabrication bug** (highest priority): numeric renderers must sample the
   *reachable* domain, not a hardcoded ±3000 box — it makes a large fraction of the
   gallery actively misleading on the (now-majority) utility programs.
2. **`fixedpoint_map` and `basin_map` should not claim "cycle"/"basin"** unless the
   structure is in the reachable set — several reported confident, *wrong* attractors.
3. **A few renderers still crash or fail** on some shapes (transition_matrix /
   state_graph missing for several programs; occupancy "N/A — transition unsat" on
   toposort = a walker-seeding bug). Track and fix or degrade to honest placeholders.
4. **Consider demoting the numeric-grid family to the continuous case only** — for
   discrete/streaming programs the reachable-set renderers carry the whole load.
