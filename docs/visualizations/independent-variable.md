# Independent-variable analysis

A relational/algebraic model has no *designed* input or output — you leave any
variable unbound and solve for it. But a model often has an **inherent** independent
variable hiding in its transition: a driver/clock that advances on its own, with the
rest computed from it. This analysis detects it (or reports that the model is
genuinely relational) and surfaces it to the programmer.

## The notion: functional-dependency asymmetry

In a typical X/Y plot, X is the independent variable and Y the dependent — meaning
*information flows X → Y*: fixing X pins Y, but fixing Y leaves X ambiguous. For a
relation that is exactly:

    Y = f(X)   holds   while   X = f(Y)   does not        (H(Y|X)=0 < H(X|Y))

A variable that **determines others without being determined by them** is acting as an
independent variable; one **determined by everything but determining nothing** is a
pure dependent. A difference-equation model almost always has such a driver — a
`cursor`/`clock`/`gen` that increments on its own — even when written to be relational.

## Algorithm

Operates on the **reachable sample** (states from `reachable()`, else a `trajectory()`);
language-agnostic — needs only the set of observed states.

1. For each ordered pair of variables `(a, b)`, test functional determination:
   `determines(a, b)` ⇔ every value of `a` maps to a single value of `b` across the
   sample (i.e. `b` is a function of `a`, `H(b|a) = 0`).
2. Net score per variable: `score(v) = #{w : determines(v, w)} − #{w : determines(w, v)}`.
   - **positive** → a driver (determines more than it's determined by) = independent.
   - **negative** → a pure dependent (a leaf — computed from others, feeds nothing).
   - **≈ 0 for all** → mutual / cyclic: no driver.
3. **Verdict:** if `max score > 0`, `driven` with `driver` = the top-scoring variable
   (tie-break: prefer a *unit counter* — distinct, consecutive integer per state — then
   the one determining the most, then the shortest name). Else `relational`.

Bijective pairs (a clock and a monotone accumulator each determine the other) cancel in
the net score, so they don't create a false driver; the asymmetry that survives comes
from a variable determining *non-injective* downstream vars (a clock → a boolean flag).

## What it found on the corpus (24 programs)

The split is clean and meaningful:

- **Driven** (an inherent independent variable = the clock): `wc`→`chars`, `grep`→`line_no`,
  `ps`/`top`/`pstree`/`ls`→`cursor`, `scheduler`→`clock`, `life`→`gen`, `toposort`→`n_out`,
  `calc`/`brackets`/`tokenizer`/`ledger`→`pos`. The pure dependents are exactly the leaves
  (`max`/`min`, never-written stack slots, the histogram bins, the Life cells).
- **Genuinely relational** (no driver): **`dungeon`, `vanderpol`, `vending`** — the
  autonomous limit cycles and the nondeterministic graph. They *loop*, so no variable
  advances independently; every variable co-determines. This is the relational design
  goal achieved, and the analysis identifies exactly where it holds.

## How it's used

- **Axis order.** When the selector's chosen pair contains the more-independent variable,
  it goes on **X** (driver/clock), the dependent on **Y** — matching the math convention.
  This composes with the structure selector: the selector picks *which* two variables (it
  discounts the clock so you don't get clock-vs-clock); independence picks the *order*.
- **A programmer-facing note** (in `viz/CONTACT_SHEET.md`): e.g. "Independent variable:
  `chars` (the driver/clock) — computed from it: `in_word`, `lines`, `words`" or
  "Genuinely relational — no independent variable."

API: `Model.independence()` in `viz/evident_viz.py` → `{verdict, driver, drivers,
dependents, score}`.

## The structural (solver-sensitivity) version — `independence_structural()`

The reachable-behavior version above infers dependencies from the sampled trajectory,
so a finite/degenerate sample can miss or invent one. The rigorous form probes the
transition RELATION directly, via the solver:

> `state.X` depends on `_state.Y` iff perturbing `_Y` — holding the rest of the previous
> state — CHANGES `state.X`.

For each of a few reachable previous states, perturb each field to another observed
value and re-solve the transition; record which next-fields move. This reads off the
actual computational form (`state.sum = _sum + val[_cursor]` responds to `_cursor`)
regardless of whether the sample exposed it. Driver = a SOURCE of the resulting DAG
(high out-degree, low in-degree — its next value is a function of itself, the rest are
computed from it).

**Determinism gate.** A nondeterministic `successor()` returns one arbitrary choice, so
perturb-vs-base would conflate dependency with that choice. When any probed state has
> 1 successor, the analysis returns `nondeterministic` — and that *is* the answer: the
independent variable is the nondeterministic CHOICE (the free input), not a state field.

## Two complementary lenses

The two methods answer different questions, and the corpus shows exactly where they
agree and diverge:

| | reachable `independence()` | structural `independence_structural()` |
|---|---|---|
| what it finds | the temporal **clock** (bijective with progress) | the computational **driver** (what propagates) |
| pipelines (cut, grep, ps, wc) | the clock | **agrees** — clock *is* the driver |
| deterministic CYCLE (vending) | `relational` (no monotone clock) | `driven: mode` — the **cyclic driver** that sequences the machine |
| nondeterministic (dungeon, du, find, randomwalk, toposort) | picks a clock-ish field | honest **`nondeterministic`** — reveals the free choice the clock view masked |
| continuous coupled (vanderpol) | `relational` | `relational` — agrees (mutually coupled, no source) |

So `du`/`find` looked "driven" to the reachable view but are actually nondeterministic
traversals — the structural probe surfaces that. And `vending`'s cyclic driver `mode` is
invisible to the clock view but caught structurally. The contact-sheet note combines
both: clock, cyclic driver, nondeterministic-choice, or genuinely relational.

A purely *syntactic* variant (parse which variable's update equation references which,
straight from the `.ev`/IR) would give the same DAG without any solving — cheaper, but
it needs an Evident expression walker; the solver-sensitivity probe gets the directed,
trajectory-independent dependency today, reusing the transition we already encode.
