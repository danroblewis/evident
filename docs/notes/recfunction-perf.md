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
budget for: **forward (call it) is cheap; backward (invert it) is expensive.** The
language can lean on RecFunctions freely for computation, but "solve for the
argument" is a deliberate, costly operation.

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
to design around: **synthesis (backward) is the expensive operation**, and **the
tactic router must blacklist the under-approximation tactics** or it will hand back
wrong answers.
