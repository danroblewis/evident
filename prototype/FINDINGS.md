# Findings — Z3 cross-theory benchmarks

Generated from `results/run.csv` (2394 cases: 7 tasks × 2 scales × N encodings ×
57 tactic sequences, `--max-len 2 --reps 2`). Regenerate with
`python3 run.py run --max-len 2`. Numbers below are the larger scale of each task.

## Headline: a slow set encoding is a missing tactic, not a wrong model

The thesis the Evident rewrite rests on — *model with sets, then let a tactic
safely lower the slow encoding* — holds, and the suite pins exactly which tactic.

`dispatch` (invert a 200-entry scrambled map), same problem six ways:

| encoding | theory | baseline | best | winning tactic |
|---|---|--:|--:|---|
| arith | int | 0.3 ms | 0.2 ms | structured arithmetic is already O(1) |
| ite | bool+int | 0.7 ms | 0.3 ms | — |
| set_bv | set+tuple+bitvec | 2.7 ms | **0.2 ms** | `blast_select_store` |
| func | uf+int | 2.8 ms | 2.6 ms | — |
| set | set+tuple | 22.5 ms | **0.5 ms** | `blast_select_store` |
| array | array+int | 169.6 ms | **0.5 ms** | `blast_select_store` |

A set-of-tuples membership is **~45×** slower than the ite chain and the array
encoding **~240×** slower — but `simplify` with `blast_select_store=True`
rewrites the `select(store-chain, symbolic-index)` into the ite spine and
recovers all of it (set 45×, array **340×**). This is the "safe lowering as a
Z3 tactic" the design predicted; `set-lowering-via-z3.md` traces it to
`array_rewriter.cpp::mk_select_core`. The combinatorial sweep *discovers* it
without being told — it is the winning sequence wherever a store-chain appears.

## No theory is universally fastest

The fast encoding flips by problem — the opposite rankings below are the whole
argument for keeping the model theory-agnostic and choosing per-problem:

- **coloring (3-colour, N=60):** Booleanizing wins. `onehot`/`enum`/`bitvec`
  all ~2 ms; `int` with `c[u] ≠ c[v]` is **13.4 ms** — the `≠` disequality is
  non-convex and case-splits, the same trap the Evident perf docs warn about.
- **arith_system (ordered sum, N=20):** the *reverse* — `real` (LRA) is 0.6 ms,
  `int` 1.4 ms, and `bitvec` is **36 ms** (worst). Bit-blasting a 32-bit sum is
  far costlier than linear real arithmetic.

Coloring says "reduce to SAT"; arithmetic says "stay in the arithmetic theory."
There is no globally correct theory — only a correct theory per problem shape.

## Reachability: bounded unroll-to-Bool beats the dedicated machinery

`reachability` (N=60), is-target-reachable three ways:

| encoding | theory | baseline |
|---|---|--:|
| unroll_bool | bool | 5.1 ms |
| special (TransitiveClosure) | relations | 9.9 ms |
| unroll_set | set | 59.2 ms |

The set-frontier encoding is **~12×** slower than the bounded Boolean unroll;
even Z3's special-relations `TransitiveClosure` is 2× slower. Bounded
unroll-to-Bool is the encoding to lower toward for reachability/planning shapes.

## Tactics can also HURT — the sweep catches it

The summary flags `ⓘ tactic-induced timeouts` where a tactic made a solvable
case time out: **11** in reachability, **3** in `fp_solve`. Floating point is
the costliest task overall (~1.3 s) and the most tactic-fragile — `solve-eqs`
+ `simplify` shaves it to ~1 s, but several sequences push it over the timeout.
Tactic application is timed *separately* from the solve, so these are real
regressions, not measurement of the tactic's own cost.

## Soundness

**Zero soundness violations** across all 2394 cases — no tactic sequence ever
changed a sat to unsat or vice versa (the summary would print `⚠⚠`). The
encodings agree on every problem, which is what licenses comparing their solve
times at all.

## Practical takeaways for the runtime design

1. **Write the model in sets/relations; lower with `blast_select_store`.** The
   pretty surface costs nothing once the tactic fires — keep the surface, change
   the lowering.
2. **Pick the theory per problem.** Booleanize finite-domain/≠ problems;
   stay in LRA for linear arithmetic; never bit-blast a wide sum.
3. **Avoid `≠` on finite domains** — it case-splits (coloring `int` 13 ms vs
   onehot 2 ms). Encode as one-hot or an enum.
4. **A tactic pipeline is part of the problem, not a free win** — measure it;
   some sequences regress (fp, reachability). The sweep is the safety net.
