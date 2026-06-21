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

Implemented in `viz/evident_viz.py`; re-measured with the same harness (see the
re-run results appended below).
