# Round 1 — Survey of Techniques

**Outcome:** 24+ techniques proposed across 4 angles. Striking convergence on
**widening the function-izer gate** as the single highest-leverage move, with
**per-FSM warm solver + assumption pinning**, **translation cache**, and **static
partial evaluation** as the next tier of compounding wins.

## What the 4 ideators said (1-line each)

### AST-level / direct-compile angle
1. Cranelift JIT for substitution chains — 6-8d, 20-40× on display
2. Per-FSM specialization (bake `_world` field offsets) — 2-3d, ~3× on tree-walker
3. Witness-replay verifier (compile spec too) — 4d, **enables aggressive gates**
4. Partial-eval of multi-FSM scheduler — 7d, ~0.5-1.5ms (Futamura)
5. Egglog/egg algebraic simplification — 5d, 2-3× via AST shrinkage
6. Trace-JIT per-input-shape — 12-15d, single-digit μs/tick

### Solver-side
1. Per-FSM warm solver + `check_assumptions` for pinning — **3-5d, 15ms → 2-5ms**
2. Translation cache (AST → Z3 Bool) — **1-2d, 1-3ms/tick**
3. Parallelize 4 FSMs per tick — 2-3d, up to 4× wall clock
4. Logic specialization (QF_LIA) — 0.5d, 1.2-1.8×
5. Tactic chain with `elim-uncnstr` — 1d, 1.3-2× (we already default to solve-eqs)
6. Z3 parameter tuning — 1d, 1.1-1.4× polish
7. Formula-level memo cache — 1d, modest

### Alternative representations
1. Static partial evaluation (const-fold pass) — **3-5d, 30-50% fewer assertions**
2. Quantifier unrolling at translate-time (AST→AST, not Z3-time) — 2-3d, **force multiplier**
3. Two-tier IR: solver form + substitution DAG for extraction — 1-2w, 3-5ms back
4. E-graph CSE via `egg` — 1.5-2w, 20-40% assertion reduction
5. Confluent term-rewriting normalizer (simpler than e-graph) — 3-4d, 10-15%
6. Finite-domain sub-solver (BDD) — 3-4w, too much effort for Mario
7. Datalog/Horn (ascent crate) — 1-2mo, scope explosion

### Caching, memoization, JIT
1. **Widen the function-izer gate from 27% to 80%+** — 3-5d, **2.5ms → ~100μs**
2. Per-chain-step memoization — 2-3d, dependent on #1
3. Differential / input-delta cache (depend-set per step) — **4-6d, 40-70% steps skipped**
4. Two-level cache (shape → program ; values → result) — 1-2d, cheap insurance
5. Frame-stable sub-expression hoisting — 1-2d, free wins
6. Persistent on-disk chain cache — 3d, startup amortization
7. Trace-based JIT to native — weeks, defer

## Cross-agent convergence

Three independent agents independently converged on:
- **Widening the function-izer / generalizing the chain extractor** (caching #1, AST broader thesis, alt-rep two-tier IR).
- **Per-FSM warm solver via Z3 incremental mode** (solver #1).
- **Partial evaluation / const-folding** (alt-rep #1, caching #5).
- **Translation cache** (solver #2).

The themes are consistent: don't replace Z3 wholesale; reduce what we *send to* Z3 (partial eval, unrolling), reuse what's *built for* Z3 across calls (warm solver, translation cache), and skip Z3 entirely for the *already-functional fragment* (widened gate, chain eval).

## Ranking by (expected_payoff × feasibility) / build_cost

| # | Technique | Effort (d) | Payoff (Mario tick) | Risk | Score |
|---|---|---|---|---|---|
| 1 | **Widen function-izer gate (Match, Field, ∀-Range)** | 3-5 | 2.5ms→100μs | low | A+ |
| 2 | **Per-FSM warm solver + assumption pinning** | 3-5 | 15ms→3ms | low | A |
| 3 | **Translation cache (AST→Z3 Bool)** | 1-2 | 1-3ms/tick | low | A |
| 4 | **Static partial eval (is_first_tick, const fold)** | 3-5 | 30-50% assertion drop | low | A |
| 5 | **Quantifier unrolling at translate-time** | 2-3 | force multiplier | low | A |
| 6 | Tactic chain with `elim-uncnstr` | 1 | 1.3-2× | low | A- |
| 7 | Parallelize 4 FSMs | 2-3 | up to 4× wall | medium | B+ |
| 8 | Logic specialization QF_LIA | 0.5 | 1.2-1.8× | low | B+ |
| 9 | Differential / input-delta cache | 4-6 | 40-70% step skip | medium | B+ |
| 10 | Two-level cache | 1-2 | small | low | B+ |
| 11 | Frame-stable hoist (specializer) | 1-2 | shrinks chain | low | B+ |
| 12 | Witness-replay verifier | 4 | enables aggressive gates | low | B+ |
| 13 | Cranelift JIT for chains | 6-8 | 20-40× on display | medium | B |
| 14 | E-graph (`egg`) CSE | 10-14 | 20-40% reduction | medium | B |
| 15 | Per-claim spec (`_world` layout) | 2-3 | 3× tree-walker | low | B |
| 16 | Two-tier IR substitution DAG | 7-14 | 3-5ms back | medium | B |
| 17 | Z3 parameter tuning | 1 | 1.1-1.4× | low | B |
| 18 | Confluent term rewriter | 3-4 | 10-15% | low | B |
| 19 | Persistent on-disk chain cache | 3 | startup only | low | C+ |
| 20 | Per-step memoization | 2-3 | depends on #1 | low | C+ |
| 21 | Formula-level memo | 1 | tiny on Mario | low | C |
| 22 | Trace-JIT | 12-15 | speculative | high | C |
| 23 | Partial-eval scheduler (Futamura) | 7 | 0.5-1.5ms | medium | C |
| 24 | BDD finite-domain solver | 21-28 | speculative | high | D |
| 25 | Datalog/ascent | 30-60 | massive but huge effort | high | D |
| 26 | rustc + libloading emit | — | dev loop poison | — | rejected |
| 27 | LLVM IR direct | — | worse than Cranelift | — | rejected |

## Round 2 pick

**Widen the function-izer gate** (technique #1) is the entry point. Reasons:

1. **All upstream infrastructure exists.** Decomposition, classification, chain
   extraction, native eval, cache, rt.query hook — all built and tested.
   What's missing is COVERAGE: the gate refuses 73% of claims because the
   evaluator can't handle them. Each expansion is mechanical.

2. **Highest single-technique payoff with smallest build cost.** Other Tier-A
   techniques (warm solver, partial eval, translation cache) are excellent but
   harder to validate one-step-at-a-time. Widening the gate has a measurable
   step (gate coverage %) AND a measurable speedup per claim that joins the
   covered set.

3. **It compounds with everything below.** Once a claim is in the
   function-izer's covered set, downstream techniques (caching, partial
   eval) compose. If we never widen the gate, the function-izer remains a
   demonstration; if we do, every other technique amplifies.

4. **Failure mode is bounded.** If we can't safely widen for some Expr
   variant, that specific widening stays unbuilt and the rest is intact.
   No risk of revert.

**Round 2 deliverable:** function-izer accepts at least one Mario FSM body
under realistic given values, with the full test suite green and a bench
showing real-program speedup.

Specifically, add support for:
- `Expr::Match` dispatch over enum scrutinees (HelloState-style state machines)
- `Expr::Field` access (player.pos.x, _world.tick)
- `Expr::Ternary` (already in eval_expr; needs gate expansion)
- `BodyItem::Membership` for user-record types and enum types (so things like
  `state ∈ HelloState` and `pos ∈ IVec2` pass the gate)
- `Expr::Forall` over Range bounds when the range is statically resolvable

Round 3 will pick the next technique based on what Round 2's measurements show.
If the gate widening lands and Mario's display FSM becomes function-shaped, the
next round goes for translation cache + warm solver to lock in solver-side wins.
If it doesn't (some FSM bodies remain irretrievably search-shaped), round 3
pivots to partial evaluation to shrink what Z3 sees.
