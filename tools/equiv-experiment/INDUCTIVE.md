# From single-tick output-equivalence to a sound inductive proof

`build-equiv-query` proves **single-tick output equivalence under φ**:

> For arbitrary shared inputs (`is_first_tick`, `last_results`, and every
> carried `_X` state dual) related by φ, the two compiler bodies never produce
> a different observable output (`effects[0..len]`, `effects__len`, every
> next-state field `X`).

That is **necessary but not sufficient** for "the two compilers behave
identically on every run." The gap: a tick's *next state* `X` feeds the *next*
tick's `_X`. Single-tick equivalence assumes the incoming `_X` are related by φ
but never re-establishes that the *outgoing* `X` are still related by φ — so it
says nothing about tick 2 onward once the state has evolved.

## The inductive obligation

Let φ relate old-state `s` to new-state `s'` (here: identity on all fields
except the 7 qloop fields, where `qloop_f ↔ qloop.f`). Write `B_old(_s, s, out)`
and `B_new(_s', s', out')` for the two tick bodies. Full behavioral equivalence
is the conjunction of two Z3 queries:

1. **Base case** (initial state). On tick 0 both start from the same declared
   initial state (here: `is_first_tick` true, fresh `_X`). Prove outputs equal
   ∧ `φ(s, s')` for the first tick. In practice the base case is *subsumed* by
   the step case when the initial `_X` are themselves related by φ (they are:
   identity on the non-qloop fields, and the qloop init values map under φ).

2. **Step case** (the real inductive query). **Assume** the incoming duals are
   related by φ:

   ```
   (assert (φ-relate  _s  _s'))        ; inductive hypothesis on the carry-in
   (assert B_old)                       ; old tick body  → defines s,  out
   (assert B_new[N!])                   ; new tick body  → defines s', out'
   (assert (or                          ; NEGATE the goal:
       (observable-output-differs out out')   ; outputs disagree, OR
       (not (φ-relate s s'))))                ; next-state leaves the φ-relation
   (check-sat)                          ; UNSAT ⇒ step holds
   ```

   `UNSAT` ⇒ *if* the carry-in states are φ-related, *then* the outputs agree
   **and** the carry-out states are again φ-related. By induction the two
   machines agree on every tick of every run.

The current `build-equiv-query` already emits everything except (a) the
`φ-relate _s _s'` **hypothesis** on the duals and (b) the `(not (φ-relate s
s'))` disjunct in the goal. Concretely:

- Today inputs are bridged with `(= _X N!_<φ(X)>)`. That IS `φ-relate` on the
  duals — so the inductive **hypothesis half is already present**. (For the
  qloop pair φ is identity-or-rename, so `(= _X N!_X)` / `(= _qloop_f
  N!_qloop.f)` is exactly φ on the carry-in.)
- The missing half is adding, to the goal disjunction, one `(not (= X
  N!<φ(X)>))` per **next-state** field — i.e. the same comparison we already do
  for the qloop next-state fields, extended to *all* carried fields, asserting
  the carry-OUT is φ-related too.

In other words: **promote every carried state field from "input only" to "input
(hypothesis) AND output (goal)".** The single-tick query in this repo already
compares the next-state fields as outputs (every non-`effects` state field is in
`outputs_old`), and bridges every dual as an input. So for a pure
rename/restructure where φ is identity-or-bijection, **the single-tick query as
built here is already the inductive step query** — the hypothesis (dual bridge)
and the goal (next-state + output disagreement) are both present.

The one caveat that keeps it from being a *fully general* inductive prover:

- **Initial-state / reachability.** The step case quantifies over *all*
  φ-related states, including unreachable ones. If the two compilers only
  differ on a state that can never actually arise, the step case could report a
  spurious `SAT`. A sound-but-incomplete fix is to strengthen the hypothesis
  with the carried **type invariants** (the kernel re-checks them every tick, so
  they hold on every reachable state) — assert each `type` body over both `_s`
  and `_s'`. That prunes unreachable counterexamples without weakening the
  proof. For the qloop rename this is moot (φ is a bijection; no state is
  differentiable), but it is the first thing to add when validating a commit
  that changes a value's *representation*, not just its name.

## Why the rename case is the easy one (and a good first target)

For `b955bdd` (qloop_ → QLoop record) φ is a **bijection** between old and new
const names with identical bodies modulo that renaming. The step query then
reduces to: "are two α-equivalent formulas equal?" — which Z3 should close
quickly *if* it scales structurally. A representation **change** (e.g. a Seq → a
record, or an int index → a key) gives a φ that is a non-trivial *relation*, and
the type-invariant strengthening above becomes load-bearing.
