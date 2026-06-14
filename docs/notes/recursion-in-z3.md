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
- **(B) The runtime owns it — a bounded unroller** (not built yet). *We* pick
  depth `N`, emit all `N` instantiations up front, one solve. **Bounded ⇒ always
  decidable/fast**, capped at `N`. This is literally "do Z3's unfolding
  ourselves, but stop at N" — and it keeps recursion in our runtime, in the fast
  bounded fragment (the benchmark suite's "bounded = fast" law at the recursion
  layer).

(B) is the natural next build: an explicit work-list that expands a self-
reference to a depth budget, the counterpart to `RecModel`, measured against (A).
Related: `docs/notes/fixed-point-models.md` (detect a tail recursion and lower it
to the transition form for memory reuse).
