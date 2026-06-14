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

### The structural fingerprint (`run-modeldiff.md`)

The model-diff report shows *why* each winning tactic helped — the operation
counts that moved. The set/array lowerings are unmistakable:

| encoding | winning tactic | what moved |
|---|---|---|
| dispatch/array | `blast_select_store` | `store 200→0`, model shrinks 408 nodes |
| dispatch/set | `ctx-simplify`>`blast_select_store` | `store 200→0`, tuple ctor `P 201→0`, −600 nodes |
| dispatch/ite | `blast_select_store`>`solve-eqs` | `if 199→0`, `= 200→0`, −603 nodes |
| reachability/unroll_set | `simplify`>`solve-eqs` | `store 10921→0`, `if 10920→0`, `select 3421→0` |
| coloring/enum | `propagate-values`>`blast_select_store` | `distinct 147→0`, `= 0→147` (distinct→equality) |

The store-chains literally vanish (`store N→0`); the model gets *smaller* and
*faster* together. `coloring/enum` is the reverse-shaped lesson — the model
*grows* (`= 0→147`) yet speeds up, because trading one `distinct` for 147 cheap
equalities is the better op-mix. Size is not the signal; the op histogram is.

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

## Reachability: the Datalog Fixedpoint engine beats every unroll — and scales

`reachability` (N=60), is-target-reachable, now five ways (the last two via the
`Fixedpoint`/`RecFunction` engines — see `docs/notes/fixpoint-engine-benchmarks.md`):

| encoding | theory | baseline | @N=120 |
|---|---|--:|--:|
| **datalog** (Fixedpoint) | fixedpoint+bitvec | **1.3 ms** | **4.8 ms** |
| unroll_bool | bool | 5.1 ms | 30 ms |
| special (TransitiveClosure) | relations | 10.2 ms | 39 ms |
| recfun (Z3-owned unroll) | recfun | 45 ms | 1235 ms |
| unroll_set | set | 59.2 ms | 1326 ms |

`Fixedpoint(engine='datalog')` saturates a `reach` relation bottom-up and is the
*fastest* encoding measured — **6×** faster than bounded unroll-to-Bool, **8×**
faster than `TransitiveClosure`, **~275×** faster than the set frontier at N=120 —
and the gap **widens** with N (datalog grows ~linearly, the unrolls
super-linearly). This overturns the earlier headline (unroll-to-Bool was the
prior winner): for a relation closure over a finite/bitvec node domain, lower to
the Datalog engine. `recfun` (Z3 owns the frontier unfolding) tracks `unroll_set`
exactly — moving the unfold into Z3's axiom doesn't change the cost; the *engine*
is the win, not who drives the unroll.

## Recursion & invariants: bound it, or let the right engine prove it

Two new tasks measure the fixpoint/recursive engines the sweep never had
(`docs/notes/fixpoint-engine-benchmarks.md`):

- **`recursion` (Σ1..n)** — a `RecFunction` is *not* free: it unfolds its axiom
  once per level, growing ~linearly (0.5 → 13.8 ms over n=20→5000) — it matches
  the bounded unroller's cost while adding semi-decidability. A runtime-emitted
  unroll stays flat (constant-folds), and a closed form is flat O(1). **Bound the
  recursion unless the depth is genuinely unknown at build time; lift to a closed
  form / fixed point when one exists** — the (A)-vs-(B) tradeoff of
  `recursion-in-z3.md`, now with numbers.
- **`invariant` (counter, safety x ≥ 0)** — `Fixedpoint(engine='spacer')` proves
  the property for ALL reachable states and is **scale-free** (~2 ms at N=20 and
  N=2000 alike), where bounded `unroll_k` grows with depth and only ever proves
  "no bad state within N". On a bug reachable only past depth N, unroll-to-50
  reports a FALSE "safe" while Spacer catches it with no depth parameter — and
  Spacer hands back the synthesized inductive invariant (`inv(A) == ¬(A ≤ −1)`,
  i.e. `x ≥ 0`). **Spacer's invariant synthesis is fast and usable** — the right
  tool for the carried-state-invariant problem (static k-induction with the
  invariant *discovered*, not supplied).

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
