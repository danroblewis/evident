# How recursion works in Z3 (and the two ways to own it)

How a recursive sub-model (`prototype/models/` `RecModel`, backed by
`RecFunction` / `RecAddDefinition`) actually executes. Measured 2026-06-14 on the
`sum_to(n, acc) = if n=0 then acc else sum_to(n-1, acc+n)` example.

## It does NOT start a new model or solver

Everything happens in **one solver, one context**. There is no nested Z3 call, no
recursive solver, no fresh model. (Z3's word *model* means *a satisfying
assignment*, not a solver instance.) Recursion is handled by adding more
constraints to the single solve in progress.

## Mechanism: lazy unfolding of the defining equation

`RecAddDefinition` does not inline anything. It registers the body as a
definitional axiom:

```
∀ n, acc.  sum_to(n, acc) = if n = 0 then acc else sum_to(n − 1, acc + n)
```

Z3 cannot expand that eagerly (infinite). A dedicated recursive-function solver
(`recfun`) **unfolds on demand**: when the search needs the value of a concrete
term like `sum_to(5, 0)`, it asserts *that one instance* of the axiom into the
current solver, which introduces a new term, which triggers the next unfold:

```
sum_to(5,0) = if 5=0 then 0 else sum_to(4,5)  → sum_to(4,5)
sum_to(4,5) → sum_to(3,9) → sum_to(2,12) → sum_to(1,14) → sum_to(0,15)
sum_to(0,15) = 15                              ← base case, chain terminates
```

So Z3 builds the **same unrolled chain we'd build by hand with `fuel`** — but
lazily, one instantiation at a time, driven by need, all within one solve. The
depth is discovered by the solver, not fixed in advance. (Proof it carries the
definition, not a value: in a model the "interpretation" of `sum_to` *is the
recursive body itself*.)

## Why it is semi-decidable (the measured trap)

The unfold depth is discovered by the solver, with no stopping rule when no
answer exists:

| query | what Z3 must do | result |
|---|---|---|
| `sum_to(5,0)` | args decrease to base — terminates | **sat, 0.7 ms** |
| find `x`: `sum_to(x,0)=15` | unfold until it hits x=5 | **sat, x=5, ~3 ms** |
| find `x`: `sum_to(x,0)=14` (no answer) | keep unfolding forever, can't prove absence | **unknown, timed out 5 s** |

The last row is the whole story: for a concrete decreasing query the chain
bottoms out (fast); when Z3 must prove *no* depth works, it has no termination
guarantee and eventually returns `unknown`.

## The two ways to own recursion

Same expansion, opposite control over when to stop unfolding:

- **(A) Z3 owns it — `RecFunction`** (built, `RecModel`). Unfolds lazily to
  whatever depth the search needs. No preset bound → answers queries you didn't
  size, but **unbounded ⇒ semi-decidable** (the `unknown` above).
- **(B) The runtime owns it — a bounded unroller** (built, `BoundedRec`). *We*
  pick depth `N`, emit all `N` instantiations up front, one solve. **Bounded ⇒
  always decidable/fast**, capped at `N`. This is literally "do Z3's unfolding
  ourselves, but stop at N" — recursion stays in our runtime, in the fast bounded
  fragment (the benchmark suite's "bounded = fast" law at the recursion layer).

## Embedding ALWAYS grows the model — so the taxonomy that follows

Embedding a recursive model — **either** (A) Z3's lazy unfolding **or** (B) the
runtime work-list — grows the model linearly with depth: one constraint and one
variable per level. In a constraint system the **memory footprint *is* the model
size**, so embedding can never give the tail-call benefit (constant memory).

| depth N | (B) `BoundedRec`: #constraints | #vars |   | transition + `run_incremental` |
|--:|--:|--:|---|--:|
| 3  | 3  | 3  |   | 4 (constant) |
| 10 | 10 | 10 |   | 4 (constant) |
| 50 | 50 | 50 |   | 4 (constant) |

A self-referential model falls into one of three cases — handle them in order:

### 1. Fixed-point reducible — the GOOD case (love this)
The model's answer is a *stable state*: a fixed point `s = step(s)` Z3 can solve
directly. Lift the fixed-point form **out** of the recursion. A fixed-point model
embedded in another is just **ordinary single-iteration model embedding** — no
unrolling, no ticks, no growth; it reads like any other embedded sub-model. Goal:
detect this and extract the fixed-point model. (Applies when you want the *stable
state*, not the path — dataflow, reachability closure, settling systems.
Sequential accumulation like `sum_to` is NOT this unless a closed form exists.)

### 2. Tail-recursive (but not a fixed point) — RUN IT SEPARATELY, as a tick
The recursive call is the **same model, in tail position, as an assertion**.
Because it's tail position the parent is effectively solved *except* for that
nested call — so instead of embedding it (growth), **re-run the same model as a
tick**: repopulate every variable with its carried value, overwriting only the
inputs passed differently. Same model, same slots → **no growth, constant memory**
(measured: footprint 4, flat across 3..50 steps). Tail position is what licenses
it: with no work pending after the call, the re-run's answer *is* the parent's
answer, so the parent can be discarded and its memory reused. (This is the
transition + `run_incremental` form, framed honestly as "re-invoke the same model
as a tick" rather than "a separate transition object.")

### 3. Neither — the HARD case (where the real work is)
General recursion: work pending *after* the recursive call. It can't be a tick
(you'd have to come back to finish the pending work) and isn't a fixed point. Two
options, both costly: **unroll** and accept O(depth) model growth, or carry an
**explicit stack** in the state (same growth, hand-managed). The growth is
fundamental here — this is the situation where neither good option exists, and
where the design effort will go.

**Decision order:** fixed-point? → tail (run as a tick)? → unroll acceptable? →
hard case. Related: `docs/notes/fixed-point-models.md` (detecting the fixed-point
and tail cases so the readable recursive surface is kept while the fast execution
is chosen underneath).
