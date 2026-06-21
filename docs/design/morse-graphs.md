# Morse graphs — rigorous global dynamics of a daemon

> Builds on [`phase-portraits-research.md`](./phase-portraits-research.md) and the
> [`phase-portraits.md`](./phase-portraits.md) thesis. The phase-portrait research
> found that our flow plotter lacks the **global skeleton** — *which* recurrent
> sets exist (fixed points, cycles, chaos) and how they're ordered. The **Morse
> graph is exactly that skeleton**, and unlike a hand-drawn separatrix sketch it is
> a **computer-assisted proof** of the global structure. This doc records the
> verified computational pipeline, its soundness guarantee, why it fits a
> solver-backed constraint language unusually well, and the build plan.
>
> Sources are the established computational-dynamics line (Conley; Mischaikow,
> Mrozek; Kalies, Ban, Gedeon, Arai, Pilarczyk; the CMGDB / DSGRN tools). The
> load-bearing claims below were cross-checked against the primary papers in a
> verified research pass (25 claims, all confirmed). Citations at the end.

---

## 1. What a Morse decomposition / Morse graph is

A **Morse decomposition** of a dynamical system is a *finite* collection of
disjoint, compact, invariant **Morse sets** {M₁,…,Mₙ} such that **all recurrent
dynamics lives inside the Morse sets**, and between them the dynamics is
**gradient-like** (a Lyapunov function strictly decreases). A Morse set
generalises "equilibrium": it captures *any* recurrent invariant set — a fixed
point, a periodic orbit, or an entire chaotic set.

The **Morse graph** is the directed acyclic graph of the reachability **partial
order** on Morse sets — formally its **Hasse diagram (transitive reduction)**.
One node per recurrent set; edges show "the system can flow from this set to that
one." Sources are repellers, sinks are attractors. It is the coordinate-free
skeleton of where the system can end up.

Annotate each node with a **Conley index** and you get the **Conley–Morse graph**
— the natural output object.

## 2. The pipeline (the established CMGDB / DSGRN method)

### Step 1 — Outer-approximate the image of each box
Discretise phase space into a grid of boxes. For each box `G`, **enclose the true
image `f(G)` in a box `▢(G)`**, then define the combinatorial multivalued map
`F(G) =` *every grid box that intersects `▢(G)`*. This guarantees

```
    f(G) ⊂ int |F(G)|      (the "outer approximation" property)
```

by construction. The classical tools compute `▢(G)` with **interval arithmetic**.
**Point sampling does not suffice** — only an enclosing *set-valued* image
guarantees the true dynamics is contained, which is what every soundness theorem
below rests on.

### Step 2 — Graph → SCC → condensation → Morse graph
Read `F` as a **directed graph** (boxes = vertices, edge `G→H` iff `H ∈ F(G)`).
Then (Ban–Kalies, linear-time):
- the **recurrent set** = boxes lying on a nontrivial cycle = the **nontrivial
  strongly connected components**;
- each SCC = one **combinatorial Morse set** (a mutual-reachability class);
- **condense** the SCCs (collapse each to a node) and take the **transitive
  reduction** → the **Morse graph** (Hasse diagram of the reachability order).

### Step 3 — Conley index → Conley–Morse graph
Classify each Morse set with the **Conley index**: an **index pair** `N=(N₁,N₀)`
(with `cl(N₁\N₀)` an isolating neighbourhood) and the induced map on **relative
homology** `H(N₁/N₀)`. In practice the tools store a **weaker, cheaper invariant**
— the nonzero eigenvalues of that homology map on the torsion-free part — which is
enough to *certify* a fixed point / periodic orbit / connecting orbit /
bistability / **chaos** (chaos = nontrivial index in degree 1 + positive
topological entropy). The index gives **sufficient** conditions: an *uncertified*
node may contain no invariant dynamics.

## 3. Why this fits Evident unusually well

- **Our query primitive *is* the outer approximation.** "`pin _state ∈ box, solve
  for the enclosure of state`" is exactly Step 1, with **Z3 playing the role
  interval arithmetic plays** in the classical tools. The set-valued "fan" I once
  called a limitation is the *required* construction.
- **There is direct precedent for building `F` by *querying a model*** rather than
  from closed-form `f` (a surrogate-model version, arXiv:2206.13779 — the identical
  "pin a box, query the image" pattern).
- **We would be in the *provably-sound* camp.** That precedent attains only
  *probabilistic* rigor (Gaussian-process confidence bounds); an **SMT/Z3 backend
  gives interval-arithmetic-grade soundness** — strictly stronger.

## 4. The soundness guarantee (stated precisely)

The guarantee is **directional**, and the exact direction matters:

- **It never misses real recurrence.** Every true fixed point / cycle / chaotic
  set lies inside some Morse set; off the Morse sets the flow is provably
  gradient-like.
- **It can produce false positives** — spurious candidate Morse sets (SCCs that
  contain no real invariant dynamics) and it can **merge** two genuinely distinct
  Morse sets into one (the over-approximation only ever *adds* connections).
- Those false positives are disposed of two ways: the **Conley index certifies**
  which candidates really contain dynamics (sufficient condition), and **grid
  refinement prunes** them — as box diameter → 0 the combinatorial recurrent set
  **converges in Hausdorff distance to the true chain-recurrent set** (Kalies–
  Mischaikow–VanderVorst, FoCM 2005, Lemma 5.7); nothing bounded away from it
  survives refinement.

So the failure mode is **false positives (spurious / merged candidates), never
false negatives (missing real structure)** — the safe direction for a tool whose
job is to *prove* the dynamics. We never wrongly claim "nothing recurrent here."
The output is a rigorous statement of the global structure **"within a given
resolution,"** refinable by subdivision.

## 5. Discrete state is EXACT — the Evident-specific win

The entire interval-enclosure apparatus exists to handle *continuous* state. **For
a finite discrete state space (enums, bools) there is no approximation at all**:
each value *is* its own box, the transition graph is **exact**, and SCC +
condensation give the **exact** Morse decomposition — zero soundness debt. The
over-approximation is only needed for the *numeric* axes.

Consequences for our examples:
- **`test_02_counter`** → one Morse set `{Done}` (the sole attractor); everything
  else is transient flow into it.
- **`vending.ev`** → one Morse set = the **6-state limit cycle**; `Idle(0,false)`
  is transient into it. The Morse graph *is* the rigorous statement "this program
  has exactly one recurrent behaviour, a cycle."

(This is the part the literature does **not** cover — see §7 — but it's the easy
direction: a finite graph needs no enclosure to be exact.)

## 6. Build plan

1. **Discrete/mixed Morse graph (exact).** Tarjan SCC + condensation on the
   reachability graph `viz.rs` already builds. Draw the Morse sets coloured on the
   portrait + the Morse graph beside it. Validates the machinery on counter/vending
   with zero approximation.
2. **Numeric Morse graph (solver enclosure).** `pin _state ∈ box` → Z3 `Optimize`
   for `▢(box)` → `F(box) =` intersecting boxes → same SCC/condensation. *Open
   cost question:* solver calls = boxes × subdivisions; and whether Z3 yields the
   box-cover directly or needs an enclosing-box extraction step.
3. **Adaptive subdivision** — refine only Morse-set boxes; this is the rigor knob
   (diameter → 0 ⇒ convergence to truth).
4. **Classification.** Start with the *topological* reading from the graph
   (no-incoming = repeller, no-outgoing = attractor, both = saddle-type); defer the
   homological **Conley index** (and revisit the center question, §7).

## 7. Honestly flagged open questions

The research verified the pipeline cleanly but could **not** close two points that
matter specifically for us:

1. **Conley index at a non-hyperbolic point (a center — our Lotka / undamped
   pendulum).** The index is *topological*, not linearisation-based, so it should
   stay well-defined where the Jacobian fails — but no source pinned down its
   *discriminating power* at a center, nor whether the cheap eigenvalue shortcut
   suffices there. Treat "handles the center" as **plausible but unverified**.
2. **Discrete / mixed state in the literature.** The whole literature is built on
   continuous grids; none of it sources the enum/bool case. We *resolve* this
   favourably (§5: finite ⇒ exact), but that's our own reasoning, not a citation.

Other practical unknowns: the cost of full relative-homology vs. the eigenvalue
shortcut; whether Z3's set-valued image can match interval arithmetic's tightness
and speed at scale.

## Sources

- **W. Kalies, R. VanderVorst**, survey (Nieuw Archief voor Wiskunde, 2016) —
  the multivalued-map / grid / poset / Morse-graph definitions for discrete-time
  systems; outer-approximation `f(G) ⊂ int|F(G)|`.
- **Z. Arai, W. Kalies, H. Kokubu, K. Mischaikow, H. Oka, P. Pilarczyk**,
  "A database schema for the analysis of global dynamics of multiparameter
  systems," *SIAM J. Appl. Dyn. Syst.* 8(3), 2009 (DOI 10.1137/080734935) — the
  automatic pipeline (outer approx → graph → combinatorial Morse decomposition →
  Conley-index classification); index-pair definition; the stored eigenvalue
  invariant.
- **H. Ban, W. Kalies**, "A computational approach to Conley's decomposition
  theorem," *J. Comput. Nonlinear Dyn.* 1(4), 2006 (DOI 10.1115/1.2338651) —
  explicit SCC-based algorithms, complexity bounds, attractor–repeller pairs,
  Lyapunov functions.
- **W. Kalies, K. Mischaikow, R. VanderVorst**, "An algorithmic approach to chain
  recurrence," *Found. Comput. Math.* 5(4), 2005 — the convergence theory
  (Lemma 5.7: `h(|R(F_Gₙ)|, R(X,f)) → 0` as `diam → 0`); no spurious persistence.
- **K. Mischaikow, M. Mrozek, F. Weilandt**, arXiv:1511.04426 — Conley indices /
  Morse decompositions of *flows* via rigorous analysis of a time-discretised
  *map* (interval arithmetic); proves the output is a valid Morse decomposition of
  the flow. Establishes discrete-map-as-primary, flow-as-generalisation.
- **CMGDB** (Conley Morse Graph DataBase, M. Gameiro; github.com/marciogameiro/
  CMGDB) — C++/Python implementation; `BoxMap(padding=True)`, `MorseGraphMap`;
  computes Conley–Morse graphs of discrete systems. **DSGRN** (github.com/
  shaunharker/DSGRN) — the switching-network specialisation. **CHomP** — the
  homology engine behind the Conley index.
- **Surrogate-model / queryable-map precedent** — arXiv:2206.13779: builds the
  combinatorial map by querying a model's confidence region (the "pin box, query
  image" pattern), classifying fixed points, periodic & connecting orbits,
  bistability, chaos. *Probabilistic* rigor — an SMT backend would be stronger.

*Verification caveat (carried from the research): several primary PDFs were
extracted manually (SIAM/ASME publisher pages returned 403); the standard
graph-theory terms "SCC / condensation / Hasse diagram / transitive reduction" are
the exact names for relations the papers define verbatim without using those
strings. The two open questions in §7 are genuine gaps, not settled facts.*
