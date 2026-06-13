# Tactics: can Z3 do the "lowering" for us?  (`t01_tactics.py`)

**The question (yours):** if the set→fast rewrite is a *safe symbolic
manipulation*, it should be a **Z3 tactic** at the solver layer, not a bespoke
compiler pass. So — does Z3's simplification turn the slow set-of-tuples form
into the fast one?

**The answer: no for the encoding choice, emphatically yes for optimization on a
good encoding.** They are two different operations, and the experiment separates
them cleanly.

## What we ran

The b01 inverse-lookup problem, `set` vs `ite` encodings, each pushed through
tactics before solving: `simplify`, `propagate-values`, `solve-eqs`,
`ctx-solver-simplify`, and a pipeline `Then(simplify, propagate-values,
solve-eqs, simplify)`. We measured tactic time, solve time, and formula size.

| encoding | N | tactic | solve_ms | size (chars) |
|---|---|---|---|---|
| **set** | 200 | none | 23.4 | 9 118 |
| set | 200 | propagate-values | 21.8 | 4 589 |
| set | 200 | PIPE | 21.2 | 4 577 |
| set | 1000 | none | 3 288 | 47 143 |
| set | 1000 | PIPE | 3 309 | 23 427 |
| **ite** | 200 | none | 0.6 | 5 239 |
| ite | 200 | simplify | 0.4 | 1 738 |
| ite | 200 | **PIPE** | **0.0** | **6** |
| ite | 1000 | none | 3.6 | 27 582 |
| ite | 1000 | **PIPE** | **0.0** | **6** |

## Findings

1. **Tactics do NOT do the set→ite "lowering".** After the full pipeline the
   `set` model is *still a 101-deep `store` chain* (the set's characteristic
   function — verified in the exported SMT2) and the **solve cost is unchanged**
   (~22 ms at N=200, ~3 300 ms at N=1000). Simplification removes redundancy
   (~2× smaller text via value propagation) but cannot convert the *modeling
   choice*. Once you've built a set-of-tuples, Z3 reasons about it as a set.

2. **But on a GOOD encoding, tactics are devastatingly effective.** On `ite`, the
   pipeline `Then(simplify, propagate-values, solve-eqs, simplify)` collapses the
   whole lookup to **6 characters** — it *solves the problem at simplification
   time* (0.0 ms, at every N). Once the encoding is right, Z3's symbolic machinery
   finishes the job, sometimes outright.

3. **So there are two distinct operations, and the terminology splits with them:**
   - **Encoding choice** (set ⇒ ite): changes *what model Z3 sees*. Z3 cannot undo
     it — it's a **modeling decision**, upstream of the solver, in the language
     layer. This *is* a real lowering (surface → constraint model); the name fits.
   - **Symbolic optimization** (`simplify`/`propagate-values`/`solve-eqs`): polishes
     the chosen model, at the **Z3 layer**, as **tactics**. *This* is the
     "safe symbolic manipulation" your instinct points at — and you're right that
     it belongs at the solver layer. It just isn't the set→ite step.

   Net: your hope ("if it's safe, it's a tactic") is correct for *optimization* and
   false for *encoding choice*. The set→ite rewrite is not a simplification Z3 can
   recover; it's a model you have to choose before Z3.

4. **Practical — we were leaving wins on the table.** Our benchmarks ran a plain
   `Solver()` with no explicit tactics. Adding the cheap pipeline is a real win on
   a well-chosen encoding (`ite`: 3.6 ms → 0.0 ms) and harmless-but-useless on a
   bad one (`set`: no change). So the workflow is: **choose the encoding (language
   layer), then run `Then(simplify, propagate-values, solve-eqs)` (Z3 layer).**

5. **Cost caveat — not all tactics are cheap.** `ctx-solver-simplify` is a
   SAT-based simplifier: **170 ms** at N=200 and **26 SECONDS** at N=1000 on the
   set — far more than the solve it precedes. The cheap workhorses are `simplify`,
   `propagate-values`, `solve-eqs`. Never put `ctx-solver-simplify` in a hot path.

## Consequence for the architecture

The optimization story is genuinely two-layer, and it matches the "we sit on top
of Z3" framing:

- The **language** owns the *encoding choice* — the surface set-relation is
  compiled to the `ite`/`array`/`func` model (this is the "lowering", and b01/b01b
  prove it's mandatory: ~1000× / timeout otherwise). Z3 can't do this for us.
- **Z3** owns the *symbolic optimization* — apply `Then(simplify,
  propagate-values, solve-eqs)` to the chosen model; on good encodings it can solve
  at preprocess time. We should run this in the harness/runtime by default.

So: keep the encoding decision above Z3, push every safe simplification *into* Z3's
tactic pipeline. Both layers, each doing the part only it can.

## Next on tactics
- Add a default `Then(simplify, propagate-values, solve-eqs)` preprocessing option
  to `bench.py` and re-measure the suite — quantify the free wins per theory.
- Probe whether a *custom* rewrite tactic (`z3.PyTactic` / a rewrite over the AST)
  could recognize "finite set-of-tuples membership ⇒ select chain" — i.e. build the
  set→ite lowering AS a tactic, testing point 3's boundary directly.
