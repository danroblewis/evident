# RecFunction performance — is "everything is a RecFunction" safe?

A language where `function`/`class` == `RecFunction` makes *every* helper a
recursive function symbol, even non-recursive ones. This is the benchmark that
checks for hidden cost. Sample problems in several forms (inline / unrolled /
closed-form vs recfun), forward and backward, swept over **every** tactic + combos
(`prototype/recfun_bench.py`, 1375 solves, `results/recfun_bench.csv`, z3 4.15.4).

## Headline: forward evaluation has NO RecFunction overhead

Baseline solve (no tactic), recfun vs the hand-written form:

| problem | recfun | other form | note |
|---|--:|--:|---|
| wrap_double (non-recursive, `dbl()` nested 12×) | **0.13 ms** | inline 0.70 ms | recfun is *faster* |
| sum_to (forward) | 0.41 ms | unroll 0.53 / closed 0.45 | comparable |
| factorial | 0.57 ms | unroll 0.51 | comparable |
| fib (branching, two self-calls) | 0.59 ms | unroll 0.56 | comparable |
| list_length (over a datatype) | 0.65 ms | — | fine |

So wrapping everything — including non-recursive helpers — in `RecFunction` is
essentially free, sometimes a win (the inline `2*(2*(…x))` nest costs Z3 *more* to
simplify than the lazily-unfolded `dbl()`). The "everything is a RecFunction"
language design pays no per-call tax for forward computation.

**Branching recursion does not blow up.** `fib` has two self-calls; naive
evaluation is exponential, but Z3 *shares* `f(n-1)`/`f(n-2)` as the same DAG term,
so `fib(18)` unfolds to ~18 distinct applications, ~0.6 ms — linear, not
exponential. (This holds for a *concrete* argument; a symbolic one is a different
story — see below.)

## The one real cost: backward solving (synthesis)

| | baseline | best *sound* tactic |
|---|--:|--:|
| sum_to **forward** (evaluate `f(30)`) | 0.41 ms | 0.15 ms |
| sum_to **backward** (find `n` with `f(n)=465`) | **106 ms** | 16.85 ms (`auflia`) |

Running a `RecFunction` *backward* — pin the output, solve for the input — is
~250× slower than forward, and no tactic gets it near forward speed (best sound
option ~6×). This is the semi-decidable synthesis cost, and it is the thing to
budget for: **forward (call it) is cheap; backward (invert it) is expensive.**

### Why backward is slow, and the workaround (confirmed against Z3's design)

Z3's design doc (`design_recfuns.md`) explains it: recursive functions are unfolded
by **iterative deepening** on an unfold-depth bound. It solves under a local
assumption `max_depth = current_max`; if the result is unsat *because of the
bound*, it raises the bound and re-solves. For a **forward** call the argument is
concrete, so the depth is determined (`sum_to(30)` needs depth 30) — one direct
unfold. For a **backward** query Z3 *doesn't know how deep to go*, so it grinds
through depth 1, 2, 3, … re-solving at each, which is the 250×. The doc confirms it
treats forward and backward uniformly — there is **no specialized inversion
mechanism and no reuse across iterations**.

This validates the plan, and answers "is there a tactic that does it for us":
**no tactic does extract-and-solve-separately** — the tactics that helped backward
(`auflia`, `smt`, …) just pick a better search engine, not a different shape. The
real fix is the runtime orchestration we sketched: **solve the rest of the system
first so the RecFunction's arguments become concrete, then forward-check the
RecFunction.** Concrete arguments = direct unfold = the 0.4 ms regime. So if a
daemon ends a recursion with a `Done` flag, you do *not* let the solver search for
the depth that makes `Done` true (backward, slow) — you determine the state, then
forward-check that the step (and `Done`) holds. "Solve everything, then check the
RecFunction's SAT with concrete inputs" is exactly the fast path.

One knob worth noting: the doc mentions a `funrec` tactic that parameterizes the
sub-strategy *and the depth bound* (e.g. `(funrec (then simplify dl smt) 100)`).
If you know the max iterations up front (a bounded daemon), setting the bound
directly sidesteps the iterative-deepening climb. Worth a follow-up benchmark.

## ⚠ The hazard: under-approximation tactics flip sat → unsat

14 of the swept results returned **`unsat` on a satisfiable problem** — a *wrong
answer*. All came from two tactics: **`add-bounds`** and **`nla2bv`**. This is not
a Z3 bug; both are documented **under-approximations** — `add-bounds` slaps
arbitrary bounds on unbounded variables, `nla2bv` rewrites nonlinear arithmetic to
fixed-width bit-vectors — and either can drop the actual solution, reporting no
model when one exists.

**Consequence for the language:** a tactic router must NOT apply tactics blindly.
A handful are unsound for a general solve and will produce wrong answers. The sound
path must **exclude the under-approximation tactics** (`add-bounds`, `nla2bv`, and
treat anything whose description says "under approximation" the same way). They're
fine *only* when you've decided an under-approximation is acceptable (fast
model-finding within a bounded space); they are never fine when the sat/unsat
answer has to be trusted. The benchmark's own "best tactic" pick of `add-bounds`
for backward solving was a *false win* — a fast wrong `unsat`.

**Two older issue-tracker claims that turned out to be FIXED in z3 4.15.4** —
flagged here because I cited them before checking, and verifying against the
running version is the whole point. Issue #5574 says `macro-finder` breaks
`define-fun-rec`; #1382 says `Optimize` gives incorrect results with recursive
functions. Directly tested on 4.15.4: **both are fine.** `macro-finder` correctly
inlines a non-recursive RecFunction *and* leaves a recursive one with the right
answer (`sum(10)=55`); `Optimize.maximize` over a RecFunction returns the correct
optimum. The benchmark agrees — `macro-finder`/`quasi-macros` returned correct
`sat` on every recfun form. So **`macro-finder` is NOT a hazard here** — it's the
function-inlining tactic, useful and safe in this version — and neither is the
optimizer. The only *verified* unsafe tactics remain `add-bounds` and `nla2bv`,
which are unsafe **by design** (under-approximations), not by bug. Lesson: an old
Z3 issue is a lead, not a verdict — test it against your actual build.

(Note: `macro-finder` is Z3's *macro-inlining* tactic — it eliminates a function
symbol by substituting its definition. It is **not** the old Evident "functionizer"
— that was the deleted kernel's Cranelift JIT that compiled extracted assignments
to native code. Different mechanisms that happen to share the word "function.")

## Tactics compose with RecFunction cleanly

Per recfun form, of ~125 tactic runs: ~113 `sat` (correct), ~10 `tactic_err`
(tactics like `split-clause` that need a clause-shaped goal — they fail gracefully
and are caught; a failed tactic does **not** poison the context for the next one),
~1 `tactic_to` (`smtfd`, the wrong tool, hits the apply timeout). No crashes, no
unfolding explosions, no context corruption. And many tactics *help* recfun
forward solves (`elim-predicates`, `qfnia` drop them to ~0.1–0.2 ms).

One operational note: tactic **apply** has no built-in timeout, so a wrong-tool
tactic can hang the sweep; bound each apply with `z3.TryFor(tactic, ms)`.

## Verdict

"Everything is a RecFunction" is safe and cheap for the common case (forward
computation, including branching recursion over concrete arguments). The two things
to design around: **synthesis (backward) is the expensive operation** — and the fix
is orchestration (solve the rest, then forward-check with concrete args), not a
tactic — and **the tactic router must blacklist the under-approximation tactics**
(`add-bounds`, `nla2bv` — unsafe *by design*) or it will hand back wrong answers.
`macro-finder` and `Optimize` are safe in 4.15.4 despite old issue reports
(verified).

## Sources

- [Z3 `design_recfuns.md`](https://github.com/Z3Prover/z3/blob/master/doc/design_recfuns.md)
  — the iterative-deepening unfold strategy; no special inversion; the `funrec`
  depth-bound knob.
- [Reynolds et al., "Model Finding for Recursive Functions in SMT" (IJCAR'16)](http://homepage.divms.uiowa.edu/~ajreynol/ijcar16a.pdf)
  — the academic basis for Z3's recfun handling.
- [Z3 issue #5574 — `macro-finder` breaks `define-fun-rec`](https://github.com/Z3Prover/z3/issues/5574)
- [Z3 issue #1382 — optimization with recursive functions yields incorrect result](https://github.com/Z3Prover/z3/issues/1382)
- [Z3 issue #2601 — `define-fun` vs `define-funs-rec` performance](https://github.com/Z3Prover/z3/issues/2601)
- [Z3 issue #7409 — `to_smt2()` omits recursive function definitions](https://github.com/Z3Prover/z3/issues/7409)
- [Online Z3 Guide — Recursive Functions](https://microsoft.github.io/z3guide/docs/logic/Recursive%20Functions/)

## Results: each row a problem, each column a solution type

Numbers are **baseline `solve_ms`** (no tactic) — the cost of handing the
form straight to a fresh `Solver`. The last column is the *fastest sound*
tactic for the RecFunction form (`total_ms`, i.e. apply + solve), excluding
the under-approximation tactics (`add-bounds`, `nla2bv`) that flip the answer.
A `—` means the form does not apply to that problem (e.g. there is no
closed form for list length, no inline expansion for a backward solve).

| problem | what it is | inline | recfun | unroll | closed | recfun + best sound tactic |
|---|---|--:|--:|--:|--:|---|
| wrap_double | non-recursive wrap, fwd | 0.7 | 0.13 | — | — | 0.07 (`auflira`) |
| sum_to_fwd | linear recursion, fwd | — | 0.41 | 0.53 | 0.45 | 0.15 (`elim-predicates`) |
| sum_to_bwd | linear recursion, **backward** (synthesis) | — | 106.03 | — | — | 16.85 (`auflia`) |
| factorial | nonlinear (mul), fwd | — | 0.57 | 0.51 | — | 0.14 (`elim-predicates`) |
| fib | branching recursion, fwd | — | 0.59 | 0.56 | — | 0.18 (`qfnia`) |
| list_length | structural (datatype), fwd | — | 0.65 | — | — | 0.12 (`qfnia`) |

Reading it: across every **forward** row the `recfun` column is within noise
of `unroll`/`closed`/`inline` (sub-millisecond) — wrapping a helper in a
RecFunction is free. The one outlier is `sum_to_bwd`: solving *for the
argument* of a recursive function is ~250× the forward cost (106 ms vs
0.41 ms), and even the best sound tactic only claws it to 16.85 ms. That is
the cost of running the recursion **backward**, not of RecFunction itself.

Per-run detail for all 1375 (problem × form × tactic) cells — apply/solve
split, `tactic_err`/`tactic_to` markers, every tactic — lives in
[`prototype/results/recfun_bench.csv`](../../prototype/results/recfun_bench.csv).

## Scaled stress: how RecFunction handles LARGE / high-recursion load

The small benchmark above is all sub-millisecond, so it can't tell us how
RecFunction degrades under load. This pass sizes the **base (no-tactic) case to
≥10 s** for each recursion shape, to find where the cost actually lives.
(`recfun_calibrate.py` / `recfun_calibrate2.py` find the scales;
`recfun_bench_parallel.py` runs the scaled sweep across cores →
`results/recfun_bench_large.csv`; `recfun_bench_large.py` is the single-process
reference for the same cells.)

### The regimes — what scales, and what doesn't

| recursion shape | how cost grows | reaches 10 s at | build (Python AST) time |
|---|---|--:|--:|
| **forward**, concrete arg (sum / fact / fib / list) | **linear in depth** | ~1.5 M unfoldings | ~0 ms (negligible) |
| **backward** synthesis (solve *for* the arg) | depth × domain | K ≈ 140 | ~0 ms |
| **wide** model (many recfun symbols, each backward) | per-symbol blowup | ~7–8 symbols | ~0 ms |
| branching backward over a *small* bounded domain (`fib(x)=T`, `x≤60`) | ~flat | never (442 ms @ T=9.2 M) | ~0 ms |

The decisive finding: **forward RecFunction evaluation never gets slow from
recursion depth alone — it is linear, and AST construction stays ~0 ms even at
3 M unfoldings** (all cost is Z3's internal lazy unfolding, not model size). A
language where every helper is a RecFunction pays nothing for *depth* on the
forward path. Cost appears only when the solver must **search**: run the
recursion backward, or juggle many recursive symbols at once. Even backward is
cheap when the *domain* is small (the `fib(x)=T, x≤60` row stays at ~0.4 s no
matter how huge the target) — it is depth × domain together that hurts.

### The scaled matrix (base case ≥ 10 s)

`base recfun` is a **clean serial** measurement (isolated, no contention);
"best sound tactic" is from the cross-core sweep (see methodology) and excludes
the under-approximation tactics (`add-bounds`, `nla2bv`) that return a *wrong*
answer (see lesson 3). Times in seconds.

| problem | scale | base recfun | best sound tactic | vs base | closed-form lowering |
|---|---|--:|---|--:|--:|
| sum_fwd  | K = 2,000,000 (forward depth) | 12.9 | 12.3 (`macro-finder`) | **1.0×** | **0.001** |
| sum_bwd  | K = 140 (backward synthesis)  | 12.5 | 11.7 (`subpaving`)    | 1.1×    | — |
| wide_sum | M = 8 symbols, each backward  | 11.7 | **0.09 (`qfnia`)**    | **~130×** | — |

Three lessons:

1. **No goal-rewrite tactic rescues forward depth.** On `sum_fwd` (2 M deep) the
   best tactic shaves ~1 %; most either no-op (solve unchanged at ~13 s), *waste*
   5–7 s on apply and still solve at ~13 s (`simplify`, `solve-eqs`), or time out
   **during apply** on the deep goal (`elim-predicates`, `ctx-solver-simplify`,
   `smt`). The unfold cost is irreducible by rewriting — the *only* thing that
   helps is changing the **encoding**: the closed-form lowering solves the
   identical problem in **~1 ms (~11000× faster)**. Principle #2 at 10-second
   scale: you do not tactic your way out, you lower.

2. **But a SEARCH-shaped load CAN be rescued by routing to the right solver
   tactic.** `wide_sum` (8 independent backward recursions) drops from 11.7 s to
   **0.09 s with `qfnia`** — a ~130× win — because the default solver interleaves
   recfun unfolding badly across the 8 symbols, while `qfnia` (quantifier-free
   nonlinear integer arithmetic) handles the bounded search directly. The same
   case is *hurt* into timeout by the wrong tactic (`qe`, `lia2card`, `fpa2bv`,
   `dom-simplify`, `occf`, `reduce-args2`). So the two regimes call for opposite
   moves: **forward depth → re-encode (lower); wide/backward search → route to a
   solver tactic.** That distinction is exactly what a shape-directed router must
   encode (and backward synthesis like `sum_bwd` sits in between — only ~1.1×
   from `subpaving`, genuinely hard to rescue).

3. **The under-approximation hazard persists — and gets more dangerous — at
   scale.** `add-bounds` flips `sat → unsat` on **all** of `sum_fwd`, `sum_bwd`,
   and `wide_sum` at 12 s: a **wrong** answer, returned *fast* (it bounds the
   variables and quickly concludes unsat under the narrowed domain). At small
   scale this looked like a curiosity; at scale it's a fast confident lie — and on
   `wide_sum` its 0.06 s "answer" would even out-rank the real 0.09 s `qfnia`
   rescue if soundness weren't checked. These tactics stay on the router
   blacklist.

### Methodology — the sweep runs across cores

The scaled sweep (252 cells) runs in `recfun_bench_parallel.py` as one process
per cell over a `multiprocessing.Pool` (default 20 workers on the 24-core box).
Process-level parallelism is what makes this safe: Z3's default context is global
*within* a process (re-declaring a recfun/sort name collides), but each child
process gets its own context, so the cells can't interfere — and the ~50-minute
serial sweep finishes in ~3 minutes. Under 20-wide load absolute `solve_ms`
inflates 10–33 % from memory-bandwidth contention (`sum_fwd` 12.9 s → 17.2 s),
which is why the `base recfun` column is re-measured serially; rankings and the
~130× / ~11000× rescue ratios are unaffected. Calibration that picked the scales
is in `recfun_calibrate.py` / `recfun_calibrate2.py`.

Per-run detail (every tactic, apply/solve split, `workers`) →
[`prototype/results/recfun_bench_large.csv`](../../prototype/results/recfun_bench_large.csv).
