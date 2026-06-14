# Fixpoint & recursive-function engines — measured against our encodings

Measured 2026-06-14, z3 4.15.4, via `prototype/benchsuite`. Three Z3 constructs
the cross-theory sweep never exercised, now benchmarked against the bounded
encodings they compete with:

1. **`RecFunction` + `RecAddDefinition`** — recursive functions, Z3-owned lazy
   unfolding (`recfun` engine). The (A) side of `recursion-in-z3.md`'s "two ways
   to own recursion".
2. **`Fixedpoint(engine='datalog')`** — least fixed point of relations over a
   finite/bitvec domain (bottom-up saturation).
3. **`Fixedpoint(engine='spacer')`** — Constrained Horn Clauses; synthesizes an
   inductive invariant over an infinite domain (the (1) "fixed-point reducible"
   case of `recursion-in-z3.md`, for *safety* rather than a value).

These don't use Goal+tactics — they call `Fixedpoint().query()` or a
`RecFunction` inside a `Solver`. The harness gained an optional `Encoding.solve`
field (a solve-only encoding brings its own bounded solve; the runner emits a
single `(none)` row, no tactic sweep). Every existing encoding/report is
unchanged; **0 soundness violations across all 2744 rows**.

Regenerate: `cd prototype && python3 run.py run --tasks reachability recursion
invariant`. Scaling tables below are from direct sweeps past the registered
scales (timeout 8 s).

## 1. Reachability — Datalog is the clear winner, and it *scales*

Same graph / S / T as the existing `unroll_*` / `special` encodings. `sat` ⇒ T
reachable (TransitiveClosure's `special` reports the dual `unsat`).

| N | datalog | unroll_bool | special (TC) | recfun | unroll_set |
|--:|--:|--:|--:|--:|--:|
| 20  | **0.5 ms** | 1.0 | 1.4 | 1.1 | 1.1 |
| 60  | **1.3 ms** | 6.7 | 10.3 | 45 | 68 |
| 120 | **4.8 ms** | 30 | 39 | 1235 | 1326 |

The Datalog engine's bottom-up saturation of the `reach` relation is **6× faster
than bounded unroll-to-Bool**, **8× faster than `TransitiveClosure`**, and
**~275× faster than the set-frontier** at N=120 — and the gap *widens* with N
(datalog grows ~linearly; the unrolls grow super-linearly). This is the first
encoding in the suite to beat `unroll_bool`, which FINDINGS had crowned the
reachability winner.

`recfun` here is the frontier-set expansion (`rk(d) = rk(d-1) ∪ succ(·)`, query
`rk(N)`) — i.e. Z3 owns the unrolling instead of the runtime. It tracks
`unroll_set` almost exactly (45/1235 ms vs 68/1326 ms): moving the unfold from
our loop into Z3's definitional axiom does **not** change the cost. The win is the
*engine* (Datalog saturation), not who drives the unroll.

**Takeaway for case-1 / reachability work:** for a relation closure over a finite
(bitvec-encodable) node domain, lean on `Fixedpoint(engine='datalog')`. It is
both faster and scale-robust, and it expresses the closure declaratively (base +
step rules) — which is exactly the set-theoretic "relations as tuple-sets"
surface we want. *Surface gap:* the node domain must be a fixed bitvec width; an
unbounded/infinite node set is not Datalog-shaped (use Spacer or unroll).

## 2. Recursion (sum 1..n) — RecFunction is NOT free; it pays per unfold

Three ways to compute `Σ 1..n`, all pinned to `n(n+1)/2` so all are `sat`:

| n | closed_form | unroll (one Goal) | recfun (Z3-owned) |
|--:|--:|--:|--:|
| 20   | 0.4 ms | 0.4 | 0.5 |
| 200  | 0.4 ms | 0.4 | 0.8 |
| 1000 | 0.4 ms | 0.5 | 2.9 |
| 5000 | 0.4 ms | 0.6 | **13.8** |

`recfun` grows ~linearly with `n` (0.5 → 13.8 ms) because Z3 lazily unfolds the
defining axiom `n` times — building, as `recursion-in-z3.md` describes, *the same
chain the bounded unroller would*, one instantiation at a time. The runtime-owned
`unroll` (emit the `n`-term `Sum` into one Goal) stays ~flat: a linear integer sum
constant-folds cheaply, so even at n=5000 it's 25× faster than recfun. `closed_form`
is flat O(1) as expected.

This sharpens the (A)-vs-(B) tradeoff from `recursion-in-z3.md`: RecFunction buys
you "answer queries you didn't size" (and semi-decidability — a non-decreasing
query returns `unknown`/timeout, measured there), but for a *concrete bounded*
recursion it is strictly slower than emitting the bounded unroll yourself, and
the gap grows with depth. **Use RecFunction only when the depth is genuinely
unknown at build time; otherwise bound it.** A closed form, when one exists, beats
both by an order of magnitude and never grows — the case-1 "fixed-point reducible
/ lift it out" advice, confirmed.

## 3. Invariant — Spacer is fast, scale-FREE, and synthesizes the proof

Transition system: `x` starts 0, step `x' = x+1`, safety `x ≥ 0`. Spacer queries
the bad state `x < 0` (`unsat` = safe for ALL reachable states) and returns the
synthesized invariant via `get_answer()`. `unroll_k` checks no bad state up to
depth N (`sat` = none found *within N* — it proves the bound, not the property).

| N | spacer | unroll_k |
|--:|--:|--:|
| 20   | 3.2 ms | 0.6 |
| 200  | 2.0 ms | 1.4 |
| 2000 | **1.9 ms** | 9.1 |

Spacer is **scale-free** — it proves the property for *all* states, so N is
irrelevant (~2 ms flat); unroll_k grows with N and still only ever covers depth N.
Spacer synthesizes the invariant directly:

```
inv(A) == Not(A <= -1)        i.e.  x ≥ 0
```

The honesty test — a system unsafe only at depth > N (bad when `x ≥ 500`):

| method | verdict |
|---|---|
| spacer | `sat` — correctly finds the bad state reachable (no depth needed) |
| unroll to depth 50  | `unsat` — "looks safe" — **FALSE**, it just can't reach the bug in 50 |
| unroll to depth 600 | `sat` — now deep enough to catch it |

Bounded unroll's "safe" is only ever "no counterexample within N"; Spacer's
`unsat` is "safe for all reachable states", and it's *cheaper* at large N because
it never unrolls. **Spacer's invariant synthesis is fast and usable** — sub-5 ms
on these systems, with a readable closed-form invariant out the back. For proving
a carried-state invariant holds for all ticks (the Evident type-invariant
problem), this is the right tool: it is the static k-induction `prove-invariants`
already gestures at, but with the invariant *discovered* rather than supplied.
*Surface gap:* writing the system as CHC rules (init clause + step clause +
bad-state query) has no Evident surface yet; it's the "transition relation as a
set of Horn clauses" shape phase 2 would need to expose.

## unknown / timeout cases

None of the new solve-only encodings produced `unknown` at the registered scales;
all three engines returned clean sat/unsat. The suite's 23 `unknown` rows are all
pre-existing tactic-induced timeouts on `unroll_set` (11) and `fp_solve` (4 ×
reps) — a tactic pushing an already-solvable Goal over the timeout, unrelated to
these engines. The known divergence traps are real but off the measured path:

- **RecFunction** is semi-decidable on a query that forces unbounded unfolding
  (a non-decreasing argument, or "prove no depth works") — returns `unknown` at
  the timeout (measured in `recursion-in-z3.md`). Our `recfun` encodings query a
  concrete decreasing depth, so they terminate.
- **Datalog** is decidable on a finite domain but the bitvec width caps the node
  count; a too-narrow width would silently wrap. We size `w = ⌈log2 N⌉`.
- **Spacer** is semi-decidable in general (CHC satisfiability is undecidable); a
  system needing a nonlinear or quantified invariant can diverge → `unknown` at
  the timeout. Our linear counter is in its sweet spot.

Every solve-only encoding bounds itself with a timeout and catches
`Z3Exception('canceled')`, recording `unknown` — the suite never hangs.

## Bottom line

- **Datalog is worth leaning on for reachability / fixed-point closure** over a
  finite node domain: fastest measured, scale-robust, declarative. It beats the
  encoding (`unroll_bool`) FINDINGS had picked as the reachability winner.
- **Spacer is worth leaning on for invariant / safety proofs**: scale-free, fast,
  and it hands back the inductive invariant. It proves for *all* states where
  bounded unroll only ever proves a depth.
- **RecFunction is the convenience option, not the fast one**: it pays per unfold
  (linear in depth), matching the bounded unroller's cost while adding
  semi-decidability. Bound the recursion yourself unless the depth is unknown at
  build time; lift to a closed form / fixed point when one exists.

Both Fixedpoint engines express the model as **relations + rules** — the
set-theoretic surface we want — and do the saturation/induction internally. They
are strong evidence that the case-1 (fixed-point) and reachability work should
target the Fixedpoint engines, not hand-rolled unrolling, *when the problem fits
their domain shape* (finite for Datalog, Horn-clause transition for Spacer).
