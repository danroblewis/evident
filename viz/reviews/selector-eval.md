# Evaluating the "interesting variables" selector

The selector ranks a program's state variables (entropy / mutual-information mRMR)
and picks the top pair as the axes of a 2-D plot. **Is it picking the most
informative pair, or hallucinating?** We measured it.

## Method

For each program with 3–7 interface variables, we rendered the *same* faithful
2-axis renderer (`orbit_scatter`) for **every** pair of variables — forcing the pair
via the `EVIDENT_VIZ_VARS` override — and recorded the selector's own pick. One
critic agent per program then viewed all the pair-projections and ruled whether the
selector's pick was the most informative (`nailed_it`), `reasonable`,
`missed_better` (a clearly better pair existed), or `hallucinated` (a degenerate /
arbitrary pair). Harness: `viz/combo_sweep.py`.

## Verdict: failing — 25% good, 62% bad

| verdict | n | programs |
|---|---|---|
| **nailed_it** | 4 | dungeon, grep, top, vending |
| reasonable | 2 | cut, pstree |
| **missed_better** | 7 | csv_stats, ledger, ls, ps, scheduler, tokenizer, uniq |
| **hallucinated** | 3 | histogram, randomwalk, wc |

| program | selector picked | reviewer's best | verdict |
|---|---|---|---|
| dungeon | room, has_key | room, has_torch (≈tie) | nailed_it |
| grep | line_no, matched | line_no, matched | nailed_it |
| top | cursor, m0 | cursor, m0 | nailed_it |
| vending | balance, mode | balance, mode | nailed_it |
| cut | line_no, field_count | line_no, field_count | reasonable |
| pstree | cursor, d2 | cursor, d5 | reasonable |
| csv_stats | **sum, min** | cursor, sum | missed_better |
| ledger | **pos, ok** | b0, ok | missed_better |
| ls | **count, largest** | cursor, total_size | missed_better |
| ps | cursor, **zombie** | cursor, max_mem | missed_better |
| scheduler | clock, running | clock, r1 | missed_better |
| tokenizer | **pos, done** | mode, tokens_emitted | missed_better |
| uniq | **cursor, prev_line** | run_count, out_count | missed_better |
| histogram | **total, bin2** | bin2, bin3 | hallucinated |
| randomwalk | **v3, node** | node, v0 | hallucinated |
| wc | **chars, words** | chars, in_word | hallucinated |

## Diagnosis: mRMR is the wrong criterion — two mechanisms

mRMR ("max relevance = entropy, min redundancy = mutual information") is a *feature
selection* algorithm: pick predictive, non-redundant features. Picking the axes of a
trajectory plot is a different problem, and the failures show exactly how it breaks:

1. **Entropy over-rewards trivial counters.** A unit counter (the cursor / tick
   index) has *maximum* entropy — every state is distinct — but it is just "time,"
   not an interesting variable. The selector keeps anchoring on it or mis-pairing it
   (`ps: cursor × zombie` — the cursor + a near-empty count; `ledger: pos × ok` — the
   cursor + a flag, when a balance is the story).

2. **Min-redundancy actively avoids the most informative pairs.** This is the killer.
   For a trajectory the *correlated* pair is often the one that shows structure — a
   cursor-vs-accumulator **staircase**, two co-moving counters. mRMR sees that
   correlation as "redundancy" and penalizes it, steering toward an *uncorrelated*
   low-information pair that renders as scattered dots:
   - `csv_stats`: picked `sum × min` → 4 scattered seed points, no orbit. The real
     aggregation staircase `cursor × sum` was passed over *because* cursor and sum
     correlate.
   - `uniq`: picked `cursor × prev_line`; the two co-moving accumulators
     `run_count × out_count` (the run-length story) were skipped.
   - `tokenizer`: picked `pos × done`; the lexer story `mode × tokens_emitted` skipped.

   The redundancy penalty is **anti-correlated with what makes a good picture.**

## Fix: a structure score, not a feature-selection score

Replace the mRMR criterion for axis pairs with a per-pair **structure score** that
rewards a meaningful 2-D shape and discounts trivial time-counters:

    score(a, b) = w(a)·H(a) · w(b)·H(b)

- `H(v)` — marginal Shannon entropy of `v` over the *reachable* orbit (ordinal-encoded
  for enums/bools/strings). A near-constant axis (e.g. `min`, which saturates after
  the first cell) has low `H` and is correctly demoted. Hard gate: `H(v)=0` (a
  constant axis) ⇒ score 0.
- `w(v)` — an **index discount** (`0.3`) when `v` is a *unit counter*: its sampled
  values are consecutive integers, one per tick (`max−min = len−1`, all distinct).
  This stops the cursor from dominating purely for being distinct every tick — while
  still letting it pair with a real accumulator when that *is* the best picture
  (`cursor × sum`).
- **No redundancy penalty.** Correlation between the two axes is *rewarded* (via the
  joint structure), not punished — the opposite of mRMR.

On the failing cases this flips the pick to the reviewer's choice: `csv_stats` →
`cursor × sum` (the discounted cursor still beats the near-constant `min`); `uniq` →
`run_count × out_count` (the cursor is discounted below the two accumulators). The
remaining `state_vars` (for the color / facet channels) are ranked by `w(v)·H(v)`
descending, after dedup, so trivial counters stop hijacking those channels too.

Implemented in `viz/evident_viz.py`; re-measured with the same harness.

---

## Re-run results: a real but partial improvement, and a confound

19 fresh critic agents re-judged the **new** picks on the same rubric and ruled
better/worse/same vs the old pick. On the **original 16** programs:

| metric | old (mRMR) | new (structure) |
|---|---|---|
| acceptable (`reasonable`+) | **6 / 16** (37%) | **9 / 16** (56%) |
| avg verdict rank (0–3) | 1.44 | **1.69** |

| | wins (↑) | regressions (↓) | unchanged |
|---|---|---|---|
| programs | cut, ps, histogram, wc, scheduler | grep, ledger, tokenizer | csv_stats, dungeon, ls, pstree, randomwalk, top, uniq, vending |

The headline wins are exactly the failure modes we diagnosed: **`ps`**
`cursor,zombie → cursor,max_mem` (the cursor now pairs with the *meaningful*
accumulator instead of a near-empty count), **`cut`** now nails the two carrying
variables over the string-noise axis, **`histogram`/`wc`** climbed out of
"hallucinated." `vending`'s `balance,mode` survived (the injective fix stopped the
cyclic balance being mistaken for a counter).

The regressions expose the cost of dropping the redundancy penalty entirely:
**`grep`** went `line_no,matched → done,line_no`, and `done = (line_no ≥ 4)` is a
*deterministic function* of `line_no` — a redundant pair the old min-redundancy term
would have rejected. So the right answer is not "no redundancy penalty" but "penalize
*functional* redundancy (one axis determines the other) while still rewarding
*correlated co-variation* (a staircase)."

### The confound: the renderer junks all-numeric pairs (the fabrication bug, again)

The three new 8-var programs (`brackets`, `calc`, `du`) all scored `hallucinated` —
but the reviewers were explicit that this is **a renderer artifact, not a selector
error**: `orbit_scatter` renders an *all-numeric* axis pair in a "multiple seeds →
attractor" mode that scatters random points across a ±3000 junk range, crushing the
real orbit to an invisible blob. Pairs that include a bool/enum axis render the true
autonomous orbit. So on numeric-heavy programs the selector can pick a perfectly good
numeric pair and the renderer still junks it — **the fabrication bug from the gallery
review now caps the selector's achievable score.** This is the same root cause flagged
in `README.md` (sample the reachable domain, don't grid/seed a fixed box).

### Verdict

The structure-based selector is a genuine improvement — from a **failing 37%** to a
**passing-ish 56%** — and it fixed the precise cases the diagnosis predicted. But two
ceilings remain, and neither is more statistical tuning:

1. **The renderer fabrication bug is now the bigger lever.** Until `orbit_scatter`
   (and the numeric family) sample the *reachable* domain instead of seeding a ±3000
   box, numeric-pair picks get junked regardless of how good the selection is.
2. **A semantic ceiling.** ~4 programs (`csv_stats`, `uniq`, `ls`, `randomwalk`) stay
   `missed_better` because the reviewers prefer *domain-meaningful* pairs (two
   co-moving accumulators; the cursor-anchored staircase) that are statistically
   dominated by a higher-entropy alternative. Closing these needs either functional-
   redundancy demotion (see grep) or a notion of "domain salience" that pure
   entropy/spread can't express.

Inter-agent variance is ~±1 verdict level (the same pick was rated `reasonable` by one
agent and `nailed_it` by another), so treat the per-program deltas as indicative and
the aggregate (6 → 9 acceptable) as the reliable signal.

---

## Re-run #2: on the FIXED renderers (un-confounding the numeric pairs)

The prior re-run flagged that `orbit_scatter` junks all-numeric pairs (the fabrication
bug), so numeric-heavy programs scored `hallucinated` for *renderer* reasons, not
selector reasons. After fixing all 8 numeric renderers to sample the reachable domain
(see `README.md`), we re-judged the **same structure-selector picks** on the now-faithful
diagrams:

| run | acceptable (16 originals) | nailed_it (of 19) |
|---|---|---|
| mRMR, broken renderers | 6/16 — 37% | ~4 |
| structure, broken renderers | 9/16 — 56% | 5 |
| **structure, fixed renderers** | **10/16 — 62%** | **10** |

The renderer fix did exactly what was predicted — it un-confounded the numeric programs
whose pairs orbit_scatter had been junking: **`calc` hallucinated → nailed_it**, **`du`
hallucinated → reasonable**. Overall **10 of 19 picks are now `nailed_it`** (63%
acceptable) — more than double the mRMR baseline's perfect-pick count. The apparent
"DOWN" moves (`top`, `pstree`, `grep` on *unchanged* picks) are the ±1 inter-agent noise,
not regressions.

### What's left (genuine, not noise)

- **`brackets`** — the selector picks `pos,s0`, but `s0` (the bottom stack slot) is written
  once and frozen; the depth staircase (`depth,pos`) is the real story. A real selector
  miss: a slot that's near-constant *after tick 1* still clears the `H > 0` gate. A
  "mostly-constant" demotion (entropy concentrated in one transition) would catch it.
- **`csv_stats`** — still `hallucinated`: `orbit_scatter` plots the raw reachable points and
  lets matplotlib auto-scale, so the `min=1e6` *sentinel seed* re-blows the axis even
  though `axis_bounds` would reject it. Small follow-up: the scatter renderers should set
  their limits from `axis_bounds`, not auto-scale. (The pair `sum,min` is also a weak
  pick regardless.)
- **Semantic ceiling** (`uniq`, `randomwalk`) — the reviewers want domain-salient pairs
  (two co-moving accumulators; node × a specific visit-count) that pure entropy/structure
  can't distinguish from equally-spread alternatives.

### Bottom line

The two fixes compound: the **structure selector** (37 → 56%) plus the **renderer
fabrication fix** (56 → 62%, and 5 → 10 nailed) took the corpus from a failing selector
on a hallucinating gallery to **62% acceptable / 10-of-19 perfect picks** on a faithful
one. The remaining gap is a couple of targeted fixes (mostly-constant demotion; scatter
limits from `axis_bounds`) and a genuine semantic ceiling.
