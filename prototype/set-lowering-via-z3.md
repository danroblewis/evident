# The set→ite lowering IS a Z3 simplifier flag: `blast_select_store`

**Result:** Z3 *can* turn the slow set-of-tuples model into the fast form, on its
own, via a documented simplifier option — no bespoke compiler pass needed. This
**corrects** the t01 conclusion ("tactics don't do the set→ite lowering"): the
*default* simplify doesn't, but the right *flag* does.

## The measurement

`dispatch/set` (set membership over a `store`-chain), with
`With(Tactic("simplify"), blast_select_store=True)`:

| N | before | after | speedup |
|---|---|---|---|
| 200 | 200 `store`, 26.8 ms | 0 `store`, an `or`, **1.0 ms** | **27×** |
| 1000 | 1000 `store`, 3285 ms | 0 `store`, an `or`, **4.6 ms** | **710×** |

The store-chain membership becomes an OR-of-equalities — the same shape as the
hand-written `ite`/`func` encodings, and the same speed. (Note: the model got
*bigger* — dag 611→808 — yet 700× faster. Op-mix beats size, again.)

So your architectural instinct was right: **the "lowering" is a safe symbolic
rewrite, and it lives at the Z3 layer as a flag.** We don't have to build it.

## How Z3 decides (from the source)

`src/ast/rewriter/array_rewriter.cpp`, `mk_select_core`, rewriting
`select(store(a, I, v), J)`:

```cpp
expr *array  = to_app(args[0])->get_arg(0);
bool is_leaf = m_util.is_const(array);
bool should_expand =
    m_blast_select_store ||                                  // (1) eager flag
    is_leaf ||                                               // (2) base of the chain
    are_values() ||                                          // (3) all indices concrete
    (m_expand_select_store && array->get_ref_count() == 1);  // (4) conservative: used once
if (should_expand)
    // select(store(a, I, v), J) --> ite(I=J, v, select(a, J))
    result = mk_ite(mk_and(eqs), v, sel_a_j);   // and-of-eqs for a TUPLE index
```

The rewrite to `ite(I=J, v, select(a,J))` fires iff **any** of:

1. `blast_select_store` — eager: always (what we turned on).
2. `is_leaf` — the array under the store is a const (the chain's base — always peeled).
3. `are_values()` — `I` and `J` are concrete values, so `I=J` is a constant fold.
4. `expand_select_store && ref_count==1` — conservative: expand only when the array
   term is used *once*, so expansion can't *duplicate* a shared subterm.

**The design decision is about term duplication.** By default (1 & 4 off), a
`select` with a *symbolic* index `J` over a *shared* store-chain matches none of
(2)(3)(4) → `BR_FAILED` → Z3 keeps the array term and reasons via the array
*theory* (lazy select-over-store axioms) — which is what's slow for our inverse
search. Z3 is conservative because eagerly blasting every shared array access can
explode the term DAG in the general case. Our experiment is precisely the case
where blasting is worth it: a single linear store-chain whose membership we want
as an explicit disjunction.

## Consequence for the architecture

The set-relation surface (`relations-as-tuple-sets.md`) can stay readable, and the
performance "lowering" can be **a solver-layer configuration, not a compiler
pass** — run `simplify` with `blast_select_store=True` before solving. Caveats,
honestly:

- It's a **global, eager** flag. On a model with *many* array accesses of mixed
  shape, blasting all of them could blow up — Z3's default conservatism exists for
  a reason. For our dispatch/registry pattern (one store-chain, membership query)
  it's a pure win; for a model that genuinely needs the array theory it isn't.
- So the clean rule: the language emits the set surface; the runtime solves with
  `blast_select_store` on **for goals dominated by finite store-chain membership**,
  off otherwise. A cheap heuristic (does the goal contain a `store`-chain selected
  at a symbolic index?) decides — and that heuristic is itself just AST inspection,
  the kind `complexity.py` already does.

## What this retires / refines

- **t01's "tactics can't lower set→ite"** — refined: the default can't; `blast_select_store` can (710×).
- **The "must build a bespoke lowering pass" worry** — softened: for the array/set
  case, Z3 already has the rewrite; we just flip the flag (and gate it by access
  shape). The *encoding choice* (write set vs write ite) is still upstream, but the
  *optimization of a set we did write* is now a known solver option.

## Reproduce
```python
g = build_set(N)
g2 = z3.With(z3.Tactic("simplify"), blast_select_store=True)(g)   # store-chain → OR
# then solve g2 — ~700× faster at N=1000
```
Profile the change with `complexity.py` (store→0, or appears).
