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
| **b01b** | *forward* lookup (chain) | same | the common compiler case vs the search case | **done ↓** |
| **b02** | finite-domain CSP (graph coloring) | Int · BitVec · Bool+cardinality · enum datatype | the "bounded under-determined" sweet spot | **done ↓** |
| **b03** | reachability / transitive closure | special-relations · Fixedpoint(Datalog) · bounded-unroll bool/sets | recursion / least-fixpoint engines | **done ↓** |
| b04 | linear & nonlinear systems | Int(LIA/NIA) · Real(LRA/NRA) · BitVec | arithmetic; the linear→nonlinear cliff | todo |
| b05 | pattern / parse | Seq · String · Regex · BitVec | sequence theory; the length-bound cliff | todo |
| b06 | cardinality / counting | pseudo-Boolean · Int-indicator-sum · (SetHasSize ⛔) | the counting gap, measured | todo |
| b07 | **mixing** | set-of-(bitvec,real) tuples w/ arithmetic; strings+int length; array-of-bitvec | the cost of crossing theory boundaries | todo |

---

> **Harness note:** `rlimit` is now **per-solve** (Δcumulative ÷ reps); the first
> commit reported Z3's *cumulative* `rlimit count` by mistake, which made flat
> encodings look like they grew. Numbers below are corrected; `min_ms` was always
> right. Runs are reps=2, 10 s timeout.

## b01 — dispatch / INVERSE lookup  (`b01_dispatch.py`)

Table `i ↦ (i*7+3) mod N`; find a key whose value is `target` — the solver must
*invert* the map (real search). Five equivalent encodings, swept N.

| encoding | N=50 rlimit/ms | N=200 | N=1000 |
|---|---|---|---|
| **arith** (formula) | 347 / 0.3 | 347 / 0.3 | **347 / 0.3** |
| **ite** (ternary spine) | 1 389 / 0.4 | 5 439 / 0.7 | **27 039 / 3.4** |
| **func** (EUF axioms) | 3 378 / 0.5 | 40 398 / 3.2 | **443 699 / 33** |
| **set** (tuple membership) | 28 536 / 5.3 | 27 480 / 23 | **436 993 / 3 409** |
| **array** (Store/Select) | 46 511 / 8.8 | 127 768 / 178 | **timeout (10 s)** |

## b01b — dispatch / FORWARD lookup, 30-step chain  (`b01b_forward.py`)

Chase the map forward `k_i = lookup(k_{i-1})`, L=30 steps from a known start — the
realistic "follow the references" pattern; the key is always determined.

| encoding | N=50 rlimit/ms | N=200 | N=1000 |
|---|---|---|---|
| **arith** (formula) | 3 115 / 0.3 | 3 115 / 0.4 | **3 115 / 0.4** |
| **array** (Select) | 3 193 / 0.3 | 4 843 / 0.4 | **13 643 / 0.9** |
| **func** (EUF axioms) | 10 400 / 1.0 | 23 150 / 1.8 | **91 121 / 6.5** |
| **ite** (ternary spine) | 21 509 / 1.3 | 79 709 / 3.3 | **412 509 / 17** |
| **set** (tuple membership) | timeout | timeout | **timeout (10 s)** |

### Dispatch findings (b01 + b01b together)

1. **`set`-of-tuples is catastrophic in BOTH directions** — ~1000× slower than
   `ite` on inverse, and it *times out* on the forward chain at every N. The raw
   set-membership form is unusable for repeated lookup. **This is the empirical
   proof of the surface-vs-lowering thesis (`relations-as-tuple-sets.md`): the
   relation is a readable *surface*; it MUST compile to `ite`/`array`/`func`, never
   run as membership.**
2. **The ternary spine was never the perf problem.** `ite` is solid in both
   directions (≤17 ms). The old codebase's guilt over ternary spines was misplaced —
   the spines are *fast*; the ugliness was readability, not speed. (Which is exactly
   why the fix is a prettier surface that lowers back to `ite`.)
3. **Direction decides `array` vs `ite`.** `array` is the *best* forward encoding
   (Select scales beautifully, 0.3→0.9 ms) but *times out* on inverse search.
   `ite` is direction-agnostic. So: arrays for forward reads, `ite`/`func` when the
   query might invert.
4. **`arith` is O(1) in table size** — flat regardless of N (the corrected rlimit
   makes this visible: 347, then 3115 for the 30-step chain). If a map has a closed
   form, encode the formula, not the table.

## b02 — finite-domain CSP, graph 3-coloring  (`b02_coloring.py`)

Planted-colorable random graph (so SAT); the solver must *find* a coloring. Four
encodings of "one of 3 colors per node, adjacent differ", swept N.

| encoding | N=20 (15 edges) | N=60 (147) | N=150 (911) |
|---|---|---|---|
| **onehot** (Bool + cardinality) | 6 965 / 0.8 | 42 056 / 2.4 | **220 388 / 9.5** |
| **bitvec** (BitVec ≠) | 4 653 / 0.6 | 38 349 / 2.6 | **381 253 / 26** |
| **enum** (datatype ≠) | 1 940 / 0.4 | 26 844 / 1.9 | **428 184 / 36** |
| **int** (Int ≠, range) | 5 132 / 1.2 | 117 095 / 11.6 | **2 498 220 / 204** |

### CSP findings

1. **Encoding matters ~20×.** Same problem, 9.5 ms (onehot) vs 204 ms (int) at
   N=150. For the "bounded under-determined" sweet spot the language wants to own,
   *how you encode the finite domain dominates*.
2. **Reduce-to-SAT wins.** `onehot` (pure Boolean + pseudo-Boolean cardinality) and
   `bitvec` (bit-blasts to SAT) are fastest — coloring *is* a SAT problem, and the
   Boolean encodings let the SAT core do what it's best at.
3. **`int` is the worst — and it's the `≠` trap.** The Int encoding leans on
   disequality (`c_u ≠ c_v`), which is **non-convex** and forces case-splits in the
   arithmetic solver; it compounds with edge count (204 ms, 10× the others). This is
   a clean re-confirmation of the old project's hardest-won lesson ("never put `≠`
   on hot state") — now visible as a 10× benchmark gap, not folklore. **For finite
   domains, encode the values as Booleans/bitvecs, not Ints with `≠`.**

## b03 — reachability / transitive closure  (`b03_reachability.py`)

"Is T reachable from S?" in a directed graph — the recursion / least-fixpoint
shape (grammars, recursive claims). The graph is split into two halves with edges
only *within* a half, so a first→second-half query is a GUARANTEED cut
(unreachable); the reachable query is a genuine multi-hop path. Fixed seed, ground
truth by BFS, **both directions measured**. Four engines:

- **special** — Z3 `TransitiveClosure(R)` over a CLOSED-world relation (assert
  `R(u,v)` for edges, `¬R` for every non-edge pair). `mk_transitive_closure`
  axiomatizes *a* transitive superset of R, not the *least* one, so the decision is
  `reachable(S,T) ⟺ ¬TC(S,T) is UNSAT` (the closure forces the pair). World-closure
  is **O(N²)** assertions.
- **fixedpoint** — Z3 Datalog/Horn (μZ): `reach(x,y):-edge(x,y)`;
  `reach(x,z):-reach(x,y),edge(y,z)`; edge facts; `query(reach(S,T))`. Closed-world
  **least** fixpoint — the textbook recursion engine. `sat ⟺ reachable`.
- **unroll-bool** — bounded BFS unrolled K=N Boolean layers: `reach_0={S}`,
  `reach_{i+1}[v] = reach_i[v] ∨ ⋁_{u→v} reach_i[u]`; ask `reach_K[T]`.
- **unroll-set** — same fixpoint, frontier as Z3 `Set(Int)` grown with `SetAdd` per
  layer (does the set theory explode on fixpoint frontiers like it did on dispatch?).

All four AGREE with BFS in both directions at every N (the equivalence check
passes). `rlimit` is the `Solver` deterministic counter; `fixedpoint` is μZ
(separate stats → wall only). reps=2, 10 s timeout, K=N for the unrolls.

**Reachable query (multi-hop path: 4 / 8 / 4 hops at N=20/60/150):**

| encoding | N=20 rlimit/ms | N=60 | N=150 |
|---|---|---|---|
| **unroll-bool** | 9 108 / 0.6 | 90 488 / 5.6 | **586 958 / 58** |
| **special** (TC) | 29 588 / 1.5 | 365 152 / 8.3 | **2 247 384 / 54** |
| **fixedpoint** (μZ) | — / 14.9 | — / 414 | **— / 3 591** |
| **unroll-set** | 9 991 / 0.5 | 232 089 / 36 | **3 701 925 / 3 699** |

**Unreachable query (the cut: S→other-half):**

| encoding | N=20 rlimit/ms | N=60 | N=150 |
|---|---|---|---|
| **unroll-bool** | 8 403 / 0.6 | 83 523 / 5.9 | **541 803 / 61** |
| **special** (TC) | 106 592 / 1.5 | 2 383 308 / 11 | **11 223 617 / 74** |
| **fixedpoint** (μZ) | — / 6.1 | — / 129 | **— / 966** |
| **unroll-set** | 8 105 / 0.4 | 177 405 / 21 | **2 597 831 / 2 395** |

### Reachability findings

1. **Bounded-unroll BOOL wins — the Fixedpoint/Datalog engine does NOT.** The
   "natural" recursion engine (μZ) is the *slowest* viable encoding in wall-clock:
   60× `unroll-bool` on the reachable query at N=150 (3 591 ms vs 58 ms). The
   least-fixpoint machinery (rule construction + μZ engine spin-up) carries fixed
   overhead that the SAT core, handed a flat K-layer Boolean circuit, simply doesn't
   pay. **For grammars / recursive claims, the answer is bounded unrolling to a
   Boolean fixpoint, not the Datalog engine.** This is the b03 analogue of b02's
   "reduce to SAT": recursion expressed as a bounded Boolean circuit lets the SAT
   core do what it's best at.
2. **`set`-frontier EXPLODES — same villain as dispatch.** `unroll-set` is identical
   in structure to `unroll-bool` but carries the frontier as `Set(Int)`; it runs
   ~60× slower (3 699 ms vs 58 ms at N=150) and its `rlimit` is 6× higher. The set
   theory is again the trap on a repeated-membership hot path — exactly the b01/b01b
   dispatch result, now confirmed for fixpoint frontiers. **A reachable-set frontier
   must be Booleans-per-node, never a `Set`.**
3. **`TransitiveClosure` is correct but O(N²)-bound to scale.** Its decision is
   right both directions, and wall-clock stays competitive (54–74 ms at N=150,
   beating μZ), but the closed-world requirement — `¬R(u,v)` for every non-edge —
   is **O(N²) assertions**, and `rlimit` shows it: 11.2M on the N=150 cut, 20× the
   `unroll-bool`. It is a relation *surface*, not a scaling engine; on a dense or
   large graph the world-closure dominates. (The open-world relation is useless: an
   uninterpreted R lets the solver invent edges, so *both* directions came back
   `sat` — the cut must be closed, and closing it is the O(N²) cost.)
4. **Direction barely matters for `unroll-bool`** (0.6/0.6, 5.6/5.9, 58/61 ms) — the
   bounded circuit pays the same whether the answer is yes or no, because it
   *computes the whole reachable set* and reads off one bit. `special` and μZ are
   cheaper on the unreachable side at small N but `special`'s O(N²) closure swamps
   that by N=150.
5. **K=N is wildly over-provisioned** (true diameter ≤ 8) yet `unroll-bool` still
   wins — so a tighter diameter bound only widens its lead. If a real diameter bound
   is known, unrolling to *that* makes the Boolean circuit smaller still.

---

## Running tally — which theory for which job

- **Dispatch / lookup:** formula if structured (`arith`, O(1)); else `ite` (any
  direction) or `array` (forward only). **Never raw set-membership.**
- **Finite-domain CSP:** **Boolean one-hot or bitvec** (reduce to SAT). Avoid `Int`
  + `≠`.
- **Recursion / reachability:** **bounded-unroll to a Boolean fixpoint** (`unroll-bool`).
  *Not* the Datalog/Fixedpoint engine (60× slower) and *not* a `Set` frontier (60×
  slower); `TransitiveClosure` is correct but O(N²) world-closure. Recursion → flat
  Boolean circuit → let the SAT core run it.
- **Cross-cutting rule:** `≠` is the recurring villain (non-convex, case-splits) —
  the same trap in the old compiler and in b02. Convex/Boolean encodings win. And
  **`Set` membership is the *other* recurring villain** — catastrophic on dispatch
  (b01/b01b) *and* on fixpoint frontiers (b03). Sets are a readable surface; they
  must lower to Bool/array/ite, never run as membership on a hot path.

Next: **b04** (arithmetic, the linear→nonlinear cliff), then **b05** (sequence /
length-bound cliff), then **b07** (mixing).
