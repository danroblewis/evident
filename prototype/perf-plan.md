# Z3 performance suite — methodology + results

**Method (yours):** take one *practical problem*, encode it in several *theories*,
and compare solve cost. Two metrics (`prototype/bench.py`):

- **`rlimit`** — Z3's deterministic work counter. Machine-independent, noise-free;
  the right discriminator (resolves orderings even when wall-clock is sub-ms).
- **`min_ms`** — wall-clock floor over reps; the practical cost at scale.

The governing variable is almost always **scale** (table size, domain size, chain
length, bound), so every benchmark sweeps it. Mixing-theory benchmarks come after
the single-theory baselines so we can read the *crossing cost*.

## The suite (problem families)

| # | problem | theories to cross | what it isolates | status |
|---|---|---|---|---|
| **b01** | dispatch / inverse lookup | arith · ite · array · EUF · set-of-tuples | membership/lookup cost; the relational-surface question | **done ↓** |
| b01b | *forward* lookup (key known) | same | the common compiler case vs the search case | next |
| b02 | finite-domain CSP (graph coloring / N-queens) | Int · BitVec · Bool+cardinality · enum datatype | the "bounded under-determined" sweet spot | todo |
| b03 | reachability / transitive closure | special-relations · Fixedpoint(Datalog) · bounded-unroll arrays/sets | recursion / least-fixpoint engines | todo |
| b04 | linear & nonlinear systems | Int(LIA/NIA) · Real(LRA/NRA) · BitVec | arithmetic; the linear→nonlinear cliff | todo |
| b05 | pattern / parse | Seq · String · Regex · BitVec | sequence theory; the length-bound cliff | todo |
| b06 | cardinality / counting | pseudo-Boolean · Int-indicator-sum · (SetHasSize ⛔) | the counting gap, measured | todo |
| b07 | **mixing** | set-of-(bitvec,real) tuples w/ arithmetic; strings+int length; array-of-bitvec | the cost of crossing theory boundaries | todo |

---

## b01 — dispatch / inverse lookup  (`b01_dispatch.py`)

Problem: table `i ↦ (i*7+3) mod N`; find a key whose value is `target` (the solver
must *invert* the map — real search). Five semantically-identical encodings, swept
over N ∈ {50, 200, 1000}, 30 s timeout.

| encoding | N=50 rlimit / ms | N=200 | N=1000 |
|---|---|---|---|
| **arith** (formula, no table) | 1 041 / 0.3 | 204 949 / 0.2 | 806 747 / **0.2** |
| **ite** (ternary spine) | 5 207 / 0.4 | 221 265 / 0.6 | 887 863 / **3.5** |
| **func** (EUF axioms) | 137 266 / 0.4 | 724 466 / 2.9 | 5 697 381 / **32** |
| **set** (tuple membership) | 203 908 / 4.1 | 805 706 / 22 | 7 002 360 / **3 411** |
| **array** (Store/Select) | 127 132 / 9.1 | 603 272 / 174 | — / **timeout (30 s)** |

### Findings

1. **Two tiers.** `arith`, `ite`, `func` are the fast tier (≤32 ms at N=1000);
   `set` and `array` are the slow tier — `set` ~1000× slower than `ite`, `array`
   *times out*.
2. **Structure beats data.** When the map has a closed form, encoding it as
   arithmetic (`(k*7+3)%N == target`) is flat ~0.2 ms regardless of N — no table at
   all. If a dispatch *has* a formula, don't tabulate it.
3. **The ternary spine is near-optimal.** `ite` is the best *table* encoding and
   scales gracefully (0.4→3.5 ms). **The compiler's `ite` chains were never the
   bottleneck** — Z3 loves them. This is a real result for the old codebase's guilt
   over ternary spines: the spines are *fast*; the ugliness was a readability
   problem, not a perf one.
4. **The set-of-tuples surface must be lowered.** Raw `(k,v) ∈ Set` membership is
   ~1000× slower than the `ite` it's equivalent to. This **empirically confirms the
   "surface vs. lowering" thesis** (`relations-as-tuple-sets.md`): write the
   relation for readability, but compile it to `ite`/`func` — never let the solver
   execute the set membership at scale.
5. **Arrays are catastrophic for *inverse* lookup.** A deep `Store` chain inverted
   by `Select(A,k)==target` blows up (timeout at N=1000). Arrays-as-maps are a
   *forward*-read tool, not a search tool — which b01b will quantify.
6. **`rlimit` vs `min_ms` mostly agree** on the big picture; where they differ
   (array has low rlimit but high wall at small N) it's construction/preprocessing
   cost, not search — worth keeping both.

### Caveat / next
This is the **inverse** (search) direction — the stressful one. The compiler's
actual dispatch is usually **forward** (key known → value), where `ite`/`array`/
`func` should all collapse to near-instant and only `set` stays slow. b01b pins
that, and it's the more representative case for "is the relational surface OK if
we lower it."
