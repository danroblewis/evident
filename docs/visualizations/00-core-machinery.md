# Core machinery — the algorithms every Evident visualization shares

> Language- and framework-agnostic spec of the shared layer that all the
> per-method visualization docs in this directory build on. Written so a future
> implementation (a web IDE, a different language) can rebuild it from scratch.
> The reference implementation is `viz/evident_viz.py` (Python + Z3) and
> `viz/contact_sheet.py`, but nothing here is Python-specific. Per-method specs
> (`phase-portrait.md`, `morse-graph.md`, …) reference this file by section.

---

## 0. Architecture

An Evident program is a **difference equation**: a transition relation over a
finite set of state variables, `state = f(_state)` (possibly *set-valued* — the
relation may admit several successors). Every visualization is a projection of
that one object. The pipeline:

```
program.ev
  → (runtime)   export transition relation as IR        [§1]
  → (viz lib)   query the transition by solving          [§2]   ← the dynamics oracle
  → (viz lib)   rank + dedup the variables               [§3]
  → (viz lib)   map variables onto visual channels       [§4]
  → (viz lib)   facet-suitability guard                  [§5]
  → (renderer)  one visualization method                 [per-method docs]
  → (driver)    interestingness-sorted contact sheet     [§6, §7]
```

The cardinal rule: **the dynamics come from *querying the transition*, never from
hardcoded logic.** A renderer that wants "where does state x go?" asks the solver.

---

## 1. The IR (intermediate representation)

The runtime emits two artifacts per program:

- **`<name>.smt2`** — the transition relation as a **self-contained SMT-LIB
  script**: datatype declarations for each enum, a `declare-fun` for every state
  leaf (`state.x`, `_state.x`, `is_first_tick`, …), and the assertions relating
  previous state `_state.*` to next state `state.*`. "Self-contained" = it parses
  standalone; no external context needed.
- **`<name>.schema.json`** — `{ fsm, is_first_tick, state: [ {name, prev, kind,
  role} ] }`. `kind ∈ {int, real, bool, enum, string}`. `prev` is the
  previous-tick constant name (`_state.x`). `role ∈ {interface, internal}`.

**A state "leaf"** is a scalar carried across ticks: a variable `X` such that both
`X` and `_X` exist. Records decompose into leaves (`state.pos.x`, …).

**interface vs internal** (`role`): the interface = the program's first-line
parameters (its observable contract). Internals are body-declared helpers —
*existentially quantified witnesses*, eliminable, not part of the observable
relation. **Visualizations default their axes to the interface**; projecting out
internals is *honest* (it's the semantics), unlike projecting out an observable.
See `../design/portrait-axes.md`.

Implementation note: in the reference runtime the interface is the first
`param_count` body items of the claim; a leaf is interface iff its dotted prefix
is a parameter name.

---

## 2. Transition queries — the dynamics oracle

Load the `.smt2` into a solver and recover, by name, the constants the schema
lists. Then the dynamics are pure solver queries. Let `pin(s)` mean "assert every
`_X = s[X]`".

- **`initial_state()`** — assert `is_first_tick = true`, solve, read the `X`'s.
  Returns one state or `None` (unsat).
- **`successor(s)`** — assert `is_first_tick = false` and `pin(s)`, solve, read
  the `X`'s. One step of `f`. Accepts *any* state `s` (it just pins the previous
  constants), so for numeric systems you may probe arbitrary grid points — you are
  not limited to reachable states.
- **`successors(s, limit)`** — the **set-valued image** (the nondeterministic
  "fan"). Repeatedly: solve, read the next state, then **assert its negation**
  (`OR_X (X ≠ value)`) and re-solve, until unsat or `limit`. This enumerates all
  distinct successors. (This is the discrete analog of the rigorous *outer
  approximation* used in Conley–Morse theory — see `../design/morse-graphs.md`.)
- **`trajectory(start, steps)`** — iterate `successor` from `start` (default the
  initial state); stop at a fixed point, a revisit, or `steps`.
- **`reachable(limit)`** — BFS/DFS from the initial state using `successors`;
  returns `(states, edges)` with `edges` the index pairs of the transition
  relation. **Exact** for finite discrete state; capped by `limit` for numeric.

Two subtleties:
- **State identity** is by the tuple of all leaf values (a canonical key), so
  cycles and revisits are detected exactly.
- **Declared-but-unused leaves**: a leaf whose *next* value ignores its *previous*
  value never appears in an assertion and the SMT parser drops it. Re-declare such
  constants by name (same sort as a sibling) so the pin/read API is uniform.

Everything downstream treats these five queries as the only interface to the
dynamics.

---

## 3. Variable ranking — dedup redundant, rank by information

Goal: produce, from the interface leaves, an **ordered, deduplicated** list — the
default axis order. Computed from a **sample of states** `S` (use `reachable()`
for discrete/mixed; a long `trajectory()` for numeric).

### 3.1 Dedup by partition equivalence ("same graph")
Each variable `v` induces a **partition** of `S`: group the sample-state indices
by `v`'s value. Two variables that induce the **identical partition** are
*informationally equivalent* — bijective on the sample, the "same graph" — and are
merged into one group (keep the most interpretable representative: prefer
enum > string > int > real > bool, then shorter name). This is **exact** redundancy
on the sample, not a correlation threshold.

(Note: this catches bijectively-related variables of any kind, including linearly
related numerics, because the partition is over index-sets, not raw values.)

### 3.2 Relevance = entropy
For each representative, Shannon entropy of its marginal on `S`:
```
H(v) = − Σ_a p(a) log₂ p(a),     p(a) = (#states with v = a) / |S|
```
Constant variables have `H = 0` (carry no information).

### 3.3 Redundancy = mutual information
For a pair:
```
I(a;b) = Σ_{x,y} p(x,y) log₂ ( p(x,y) / (p(x) p(y)) )
```
Normalized redundancy `r(a,b) = I(a;b) / min(H(a), H(b)) ∈ [0,1]` (0 = independent,
1 = one determines the other). Type-uniform — works for enum/bool/int alike,
unlike Pearson correlation.

### 3.4 Greedy mRMR ordering
Order the representatives by **max-relevance / min-redundancy** (Peng et al. 2005):
1. first = the highest-entropy variable;
2. each next = `argmax_v  H(v) · (1 − max_{p already chosen} r(v, p))`.

The result: any prefix of the list is an informative, mutually-non-redundant set;
`ranked[:2]` is the most expressive *axis pair*; `ranked[:k]` a good `k`-set.

Degenerate guard: if the sample has `< 2` *distinct* states (e.g. a numeric
program whose initial state is a fixed point), skip dedup/ranking and return the
raw interface order.

---

## 4. Channel mapping — variables onto visual channels

A visualization is a set of **visual channels**, each effective for different
variable **types** (Bertin 1967; Cleveland & McGill 1984; Mackinlay 1986). The
empirical ranking: **position** (axes) decodes best for everything; **size** is
good for *quantitative*, poor for *categorical*; **color (hue)** and **facet** are
excellent for *categorical*, weak for *quantitative*. Encode this as a fitness
table (rows = channels, cols = variable class), values in `[0,1]`:

| channel | quantitative (int/real) | categorical (bool/enum/string) |
|---|---|---|
| `x`, `y` (position) | 1.00 | 0.90 |
| `size` / `opacity` | 0.70 / 0.60 | 0.25 |
| `color` (hue) | 0.40 | 0.85 |
| `facet` (small multiples) | 0.20 | 0.80 |
| `shape` | 0.10 | 0.60 |

**Assignment** (`assign_channels(channels)`): greedily, in variable-importance
order (§3.4), place each variable on its **best free channel for its type**:
```
for v in ranked_vars:
    cls = "cat" if v is bool/enum/string else "quant"
    candidates = { free channels with fitness[ch][cls] ≥ min_fit }     # min_fit ≈ 0.3
    if none: continue                       # skip v; a later var may fit
    best = argmax over candidates of (fitness[ch][cls], earlier-declared)   # tie → x before y
    assign best ← v;  mark best used
```
The `min_fit` floor prevents nonsense (a categorical never lands on `size`, 0.25).
Color / size / facet are **secondary**: a good plot must read from its **axes
alone**; they only enhance. Renderers that *require* numeric axes (vector fields)
use the ranked **numeric** sublist directly rather than `assign_channels`.

Categorical variables on a position axis are **ordinal-encoded** (enum → index in
its variant list, with the variant names as tick labels; bool → 0/1).

---

## 5. Facet-suitability guard

Faceting = small multiples, one panel per value of a variable. It **adds a
dimension honestly** — *but only if the faceted variable is roughly constant
within a run*. Faceting by a variable that is itself *on the trajectory* (e.g. a
mode that cycles every tick) cuts exactly the transitions you want to see and
destroys the dynamics.

**Change rate** of a variable = fraction of transition edges along which it
changes value:
```
change_rate(v) = (# edges (i,j) with states[i][v] ≠ states[j][v]) / (# edges)
```
computed over `reachable()`'s edges (or consecutive `trajectory()` states for
numeric). A **config/regime** variable (set once, like an item picked up) has a
low rate; a variable on a cycle has a high rate.

`facet_var()` returns the facet variable, or `None` (then **don't facet**): a
**categorical** of low cardinality (`≤ ~6` values) with
`change_rate ≤ ~0.25`, choosing the most static (lowest change rate). Example:
a vending machine's `mode` (rate 0.57) is rejected → single plot, cycle visible;
a dungeon's `has_treasure` (rate 0.02, picked up once) is accepted → a meaningful
before/after facet.

---

## 6. Interestingness — sorting the contact sheet

Each rendered image gets a scalar so degenerate/blank diagrams sink and busy ones
float. The reference metric is **visual-content / busyness**, background-agnostic:
```
score = stddev(grayscale pixels) + 8 · mean( |∇ grayscale| )      # contrast + edge density
```
The contact sheet sorts each program's diagrams by this, descending.

**Known limitation (documented honestly):** busyness rewards *density*, not
*insight* — dense heatmaps float, clean sparse graphs sink. It does the **floor**
job (sinks genuinely degenerate diagrams) but not the **ceiling** (surfacing the
*insightful* ones). The intended upgrade is data-and-viz-aware: a **per-viz fit**
score (does this renderer's assumptions match this program's shape?) × a
**structure-revealed** score (a limit cycle, multiple attractors, the chosen
pair's joint structure), with busyness kept only as a degeneracy floor.

---

## 7. The contact-sheet generator

A driver that, over an **extensible list of sample programs** and **auto-discovered
renderers**:
1. exports each program's IR (§1) — parallel;
2. runs every `(program × renderer)` in a **worker pool** — each renderer is a
   standalone process `render <smt2> <schema> <out>`; collects success + the
   interestingness score (§6);
3. writes a **markdown contact sheet grouped by program**, each program's diagrams
   laid out as a table (N per row, padded so a lone diagram keeps its width),
   sorted by interestingness, with a one-line state description.

Adding a program = one line in the sample list; adding a visualization = drop a
new renderer (auto-discovered). Generated images/IR are disposable (regenerated by
the driver), not committed.

---

## References

- **Dynamical systems**: Strogatz, *Nonlinear Dynamics and Chaos* (difference
  equations, phase portraits, cobwebs). Conley–Morse computational dynamics for the
  set-valued / reachable-graph view (`../design/morse-graphs.md`).
- **Information theory**: Shannon entropy; mutual information; Peng, Long & Ding,
  "Feature selection based on mutual information: mRMR" (IEEE TPAMI 2005).
- **Redundancy as partition equivalence**: functional dependency / the partition
  lattice over the sampled relation.
- **Visual encoding**: Bertin, *Semiology of Graphics* (1967); Cleveland & McGill,
  "Graphical Perception" (JASA 1984); Mackinlay, "Automating the Design of
  Graphical Presentations" (ACM TOG 1986) — the APT effectiveness ranking that §4
  encodes.
- **Companions**: `../design/portrait-axes.md` (interface = axes, witnesses),
  `../design/visualization-survey.md` (the landscape), `../design/phase-portraits-research.md`,
  `../design/morse-graphs.md`.
