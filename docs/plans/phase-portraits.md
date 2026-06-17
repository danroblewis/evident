# Phase portraits: visualizing a daemon's state space (and why the proof *is* the picture)

> Forward-vision design note. Sits alongside [`claims-as-sets.md`](./claims-as-sets.md)
> and [`relations-as-tuple-sets.md`](./relations-as-tuple-sets.md). The thesis:
> a daemon written as a constraint system is a **dynamical system**, its behavior
> is a **flow over a feasible region**, and the picture of that flow — the *phase
> portrait* — is simultaneously how a human understands the daemon, how they debug
> it, and what a safety/liveness proof *is*. Visualization and verification are one
> object seen two ways. This note argues that the phase portrait is the right
> organizing idea for the language's surface, its IDE, and the very definition of
> "what goes in a claim."

---

## Why this matters for the language

We are designing a relational, "state-things-you-wish-were-true" language whose
runtime is Z3 + a minimal harness, aimed at **provable daemons**. Two questions
have dominated the design:

1. **What goes in a claim?** (the unit of the language)
2. **How does a human read, understand, and debug a daemon** written this way?

The claim this note makes is that *both questions have the same answer*, and the
answer is geometric: **a claim is the smallest thing that has its own legible
phase portrait, and what goes in it is exactly what that portrait needs.** If you
can draw a claim's behavior in isolation, you can understand it in isolation —
and local understandability is the only thing that lets a declarative system
scale past toy size.

---

# Part I — Intuition

## A daemon is a thing that moves

Forget constraints for a second. A long-running program has some **state** — a
handful of numbers and flags. At any instant the state is a single **point**: if
the state is `(queue_depth, heater_on)` then "queue 3, heater off" is the point
`(3, 0)`. The space of all possible such points is the **state space** (a.k.a.
phase space). The program *running* is that point **moving** — each tick it hops
to a new point.

A **phase portrait** is the picture you get when you draw, for many starting
points, *where the state goes*. You sample a cloud of points and from each draw a
little arrow to its next-tick successor. Step back and the arrows form a **flow** —
like iron filings around a magnet, or wind on a weather map. That flow *is* the
program's behavior, shown all at once instead of one run at a time.

This is an old, deep idea from physics and mathematics (Poincaré's qualitative
theory of differential equations): you often learn far more about a system by
looking at the *shape of its flow* than by solving for any single trajectory.

## The two halves: the cloud and the flow

A daemon-as-constraints has two parts to draw, and you need both:

- **The cloud (static).** The set of states that are even *allowed* — the
  feasible region, the solutions of the state constraints. This is what an earlier
  prototype IDE already showed: per-variable ranges, pairwise scatter plots,
  sampled points. It answers *"where can the system be?"*
- **The flow (dynamic).** The transition overlaid on the cloud — arrows from each
  state to its successor(s). It answers *"where does the system go?"*

The old viewer had the cloud. The missing half — the thing that turns a
constraint browser into a *daemon-comprehension tool* — is the flow.

## What you can suddenly see

Once the flow is drawn, the failure modes that are invisible in source code
become *obvious shapes*:

- **A fixed point** — arrows converging to one spot: the system settles here and
  stays (`state = step(state)`).
- **A limit cycle** — arrows looping forever: that is **livelock**, finally
  visible as a circle.
- **A sink** — a state you can enter but whose only "successor" is nothing: that
  is **deadlock**.
- **A region the flow never leaves** — that is an **invariant**, a safety
  property, drawn as a box (or blob) the arrows never cross.

The hard-to-see daemon bugs — deadlock, livelock, starvation, leaks — are not
hard to see in a phase portrait. They are *topology of the flow*.

## The punchline: the proof is the picture

Here is the part to build the language around. Everything a safety/liveness
prover computes is a feature of this same picture:

- The **inductive invariant** Spacer synthesizes (e.g. `0 ≤ q ≤ CAP`) is literally
  a **box drawn around the flow** — the region the arrows provably never escape.
- The **safety proof** is the **reachable set growing into that box and stopping**:
  tick 0 a point, tick 1 a small blob, … expanding but never crossing the wall.
- A **counterexample** ("here's how you reach the bad state") is a **single
  trajectory that escapes the box** — drawn, highlighted, walkable step by step.
- **Liveness** ("every request is eventually served") is the flow **descending**
  into a goal region from everywhere.

So the picture is not decoration laid on top of a proof. **The picture is the
proof, made perceptible.** That coincidence — that the thing a human needs to see
and the thing a solver needs to find are the same geometric object — is the whole
reason this is worth designing around.

---

# Part II — Theory

## II.1 Dynamical-systems vocabulary (the precise version of Part I)

A **discrete-time dynamical system** is a set `X` (the **state space**) and a map
`f : X → X` (the **transition** / **time-one map**). A **trajectory** (or orbit)
from `x₀` is the sequence `x₀, f(x₀), f²(x₀), …`. The **phase portrait** is the
collection of trajectories, usually summarized by the **vector field** `x ↦ f(x)`
(or `x ↦ f(x) − x`, the displacement) drawn over `X`.

Key structures (Strogatz; Hirsch–Smale–Devaney):

- **Fixed point**: `f(x*) = x*`. The system rests there.
- **Periodic orbit / limit cycle**: `fᵏ(x*) = x*` for some period `k`.
- **Attractor**: a set the flow converges *into*; its **basin of attraction** is
  the set of start points that end up there.
- **Repeller / source**: the flow moves away.
- **Invariant set** `S`: `f(S) ⊆ S` — once in `S`, always in `S`. (This is the
  geometric heart of "safety," below.)
- **Lyapunov function**: a scalar "height" `V : X → ℝ≥0` that the flow *descends*
  (`V(f(x)) < V(x)` off the target). Its existence proves convergence/stability.
  The discrete, well-founded analogue is a **ranking function** — and that
  equivalence (Lyapunov ⇔ ranking function) is the bridge from control theory to
  termination/liveness proofs (Podelski–Rybalchenko).

## II.2 Translating to a constraint system

Our daemon is not a clean function `f`. It is a **relation**, because synthesis
deliberately leaves choices open (the C-style "free" variables). So:

- **State space** `X` = the product of the carried variables' domains.
- **Feasible region** = the solution set of the state (invariant) constraints — a
  subset of `X`, generally non-rectangular (constraints couple the axes).
- **Transition** = a **relation** `R ⊆ X × X`, *set-valued*: a state may have many
  legal successors (`R(x, x′)` under-determined). The "flow" is therefore not a
  vector field but a **field of cones/fans** — from each point, a *set* of arrows.

The set-valued case has its own mature theory: **differential inclusions** and
**viability theory** (Aubin & Cellina; Aubin). The objects we care about:

- **Post-image** `post(S) = { x′ | ∃x∈S. R(x,x′) }` — one step forward from a set.
- **Reachable set** `Reach = ⋃ₙ postⁿ({init})` — the forward orbit of the initial
  set; the *true* cloud the daemon can occupy over all time.
- **Controlled-invariant set / viability kernel**: the set of states from which a
  *safe successor always exists* — i.e. from which the synthesizer can keep the
  invariant forever. **This is exactly the "is the C-style synthesis even
  well-posed?" question** from the design discussion: a free-choice daemon can run
  forever iff its start states lie in the viability kernel. Geometrically: the
  largest sub-region of the safe set that the flow can be *kept* inside.

## II.3 Invariants and reachability, as geometry

A predicate `Inv ⊆ X` is an **inductive invariant** for `(init, R, Bad)` iff:

```
  init ⊆ Inv                 (the start is inside)
  post(Inv) ⊆ Inv            (closed under one step — a trapping region)
  Inv ∩ Bad = ∅              (it avoids the bad set)
```

Geometrically `Inv` is a **trapping region** that contains the start and misses
the danger. Safety holds iff such a region exists. This is precisely what IC3/PDR
(Bradley; Een–Mishchenko–Brayton) and its SMT generalization **Spacer**
(Komuravelli–Gurfinkel–Chaki) search for, and what we have repeatedly watched
Spacer synthesize sub-second for our daemons. The dual, **backward reachability**,
computes `preⁿ(Bad)` — the states that *can* reach danger; safety is `init`
disjoint from it.

## II.4 The correspondence — the centerpiece

| daemon concept | phase-portrait picture | formal-methods object |
|---|---|---|
| state vector | a point | a valuation of the carried variables |
| one tick | one arrow of the flow | post-image under the transition `R` |
| nondeterministic / free choice | a *fan* of arrows from a point | under-determined relation `R(x, x′)` |
| feasible states | the cloud / region | solution set of the state constraints |
| safety invariant | a trapping region the flow never leaves | inductive invariant (`init⊆I`, `post(I)⊆I`, `I∩Bad=∅`) |
| **safety proof** | reachable set grows into the box and stops | IC3 / PDR / Spacer invariant |
| **counterexample** | a trajectory escaping to `Bad` | BMC / Spacer cex trace |
| deadlock | a sink with no outgoing arrow | a state with no enabled transition |
| livelock | a limit cycle missing the goal | a reachable cycle with no progress |
| liveness / progress | every orbit descends to the goal region | ranking function / well-founded measure |
| "can stay safe forever from here" | the viability kernel | winning region of the safety game |
| stability of a resting state | a basin of attraction | the set that converges to the fixed point |
| a Lyapunov "height" the flow descends | contour lines the flow crosses downward | a ranking function (discrete Lyapunov) |

The left two columns are how a *person* reads the daemon. The right column is what
a *solver* computes. They are the same objects. That is the unification.

## II.5 The *shape* of the region — and why our benchmarks predicted it

The trapping region is rarely an arbitrary blob; we *approximate* it with a shape
from a fixed menu, and the menu is the catalog of **abstract-interpretation
numeric domains** (Cousot & Cousot). Each domain is a family of region-shapes with
a precision/cost trade-off:

| domain | region shape | expresses | cost |
|---|---|---|---|
| **intervals / box** | axis-aligned box | `lo ≤ x ≤ hi` | cheapest |
| **octagons** | box + 45° cuts | `±x ± y ≤ c` (**difference bounds**) | cheap (Miné) |
| **zonotopes** | centrally-symmetric polytope | linear images of a box | medium (Girard) |
| **convex polyhedra** | arbitrary convex polytope | `Σ aᵢxᵢ ≤ c` (incl. equalities/**sums**) | expensive (Cousot–Halbwachs) |
| **non-convex / disjunctive** | unions, ellipsoids, … | conservation, modular, multi-modal | hardest |

This directly **explains a measured result** from our own suite
([`recfunction-perf.md`](../notes/recfunction-perf.md), and the pipeline/cache
Spacer experiments): Spacer nails **difference-bounded** invariants (`0 ≤ q ≤ CAP`,
`q0 − q1 ≤ k`) in milliseconds — those are *octagon-shaped* regions — but
**diverges on conservation/sum** invariants (`a + b + c = TOTAL`), which need the
*polyhedral* (equality-carrying) shape. The geometry of the invariant region is
the cost model. A language that knows which shape a claim needs knows in advance
whether its proof will be cheap — and the IDE can *draw the shape it's reaching
for*.

## II.6 Abstraction and scale: the quotient portrait

For large or discrete state spaces the concrete portrait explodes. The fix is
**predicate abstraction** (Graf–Saïdi): partition `X` by a handful of predicates
(`q=0`, `q=CAP`, `heater_on`, …) and draw the **quotient** — one node per region,
edges where some concrete transition crosses. You get a small, legible
*abstract* phase portrait. **CEGAR** (Clarke et al.; Ball–Rajamani's SLAM)
refines the predicates when the abstraction is too coarse to settle a question.
The design reading: *the predicates a user cares about are the axes of the portrait
they want to see*, and abstraction lets the same daemon be viewed at many zoom
levels.

## II.7 Getting the points: sampling, and a soundness caveat

To draw the cloud you need points, and **how** you get them changes what the
picture *means*:

- **Sampling** (enumerate / sample satisfying assignments) gives **real, witnessed
  points** but an **under-approximation** — it shows *existence* ("this state *is*
  reachable/feasible"), never completeness. Uniform sampling of a solution set is
  itself hard: naive model enumeration clusters; near-uniform requires
  hashing-based samplers (UniGen — Chakraborty–Meel–Vardi) or, in the convex case,
  random walks (hit-and-run; the Dikin walk — Kannan–Narayanan). Model *counting*
  (Barvinok / LattE; #SAT) gives the region's volume / a variable's mass.
- **Invariant synthesis / abstract interpretation** gives an **over-approximation**
  — a region guaranteed to *contain* every reachable state. It shows *universality*
  ("the system can *never* leave this box").

**The soundness directions are opposite, and the UI must not blur them.** For a
**safety** claim you want the *over-approximation* (the proven box that contains
the flow). For a **"can this happen?"** / counterexample claim you want a
*witnessed sample* (a real escaping trajectory). Drawing sampled points as if they
were the whole story would be a *false-safe* lie; drawing an over-approximation as
if every point in it were reachable would be a *false-alarm* lie. The honest
portrait renders **proven region** (outline) and **witnessed samples** (dots) in
visibly different ink.

## II.8 Dimensionality: projections lie, the constraint graph tells you where

Past ~3 state variables you must **project** to 2-D/3-D, and projection *omits*:
two variables can each be in-range while the *pair* `(x, y)` is forbidden — that
hole is invisible in the per-variable marginals and shows only in the joint plot.
So no single view suffices. Tools that help:

- **Pairwise joint regions** for *coupled* pairs — the 2-D shape *is* the
  constraint between them (a diagonal band = `x ≈ y`; a missing corner = an
  exclusion).
- **The constraint graph** (variables = nodes, an edge where a constraint couples
  them) is the **map of which projections matter** and, crucially, reveals
  **separable clusters** — independent sub-graphs are the daemon's natural modules
  (and, per Part III, its claim boundaries).
- **Parallel coordinates** (Inselberg): each variable an axis, each sampled
  solution a polyline across all axes — the standard way to see *all* dimensions of
  a high-D solution set at once.
- **Dimensionality reduction** (PCA, and with care UMAP/t-SNE) for continuous
  clouds — useful but distorting; honest only with the caveat that distances and
  holes can be artifacts.

---

# Part III — Design implications for the language

## III.1 The claim is the unit of the picture — and that defines what's in it

If understanding a daemon means reading its phase portrait, and portraits don't
scale globally (Part II.8), then **you render per claim and compose.** That forces
a definition of the claim that is also the answer to "what goes in a claim":

> **A claim is the smallest unit that has a self-contained phase portrait. Its
> boundary contains exactly what is needed to draw that portrait without opening
> any other claim:**
>
> | claim ingredient | portrait role |
> |---|---|
> | **carried variables** | the **axes** |
> | their **realistic ranges** | the **box** (per-axis extent) |
> | the **couplings/constraints** among them | the **shape** of the cloud |
> | the **transition** | the **flow** |
> | the **invariant** | the **trapping region** the flow stays in |
> | **assumed inputs / emitted effects** | the picture's **edges** (where the world enters/leaves) |

This is not a metaphor to be pretty about; it is a checklist with teeth. If those
six things are on a claim's boundary, the claim is **locally renderable**, hence
**locally understandable**, hence **composable** — you glue two portraits along
their shared axes via the contracts, never re-deriving the whole. Local
renderability *is* local reasoning (the property declarative/constraint languages
lose by default and must buy back).

## III.2 Degrees of freedom are *visible* in the portrait

In a synthesis language the reader's first question is "what's pinned vs what does
the solver choose?" The portrait answers it geometrically:

- a **pinned** variable is a **point** (zero spread),
- a **free** variable is a **spread** (the cloud's width along that axis),
- a **preferred** (soft-optimized) variable is a **density gradient** (the cloud is
  denser where the objective is happier).

So "degrees of freedom," the thing we said must be the most visible element of the
syntax, *is* the most visible element of the picture — the literal width and
density of the cloud.

## III.3 The authoring loop and the new debugging primitives

You cannot mentally simulate a constraint daemon; you **interrogate** it. The
natural loop is *state a wish → look at the portrait → over/under-constrained? →
refine*, and the portrait is the feedback surface:

- **Over-constrained ⇒ UNSAT ⇒ the cloud goes empty.** The portrait blanks out.
  The diagnostic is the **unsat core** — *which* constraints collapsed the region —
  shown as the offending walls closing to nothing.
- **Under-constrained ⇒ the cloud is too big / the flow sprawls.** Erratic behavior
  is a visibly sprawling fan of arrows.
- **Vacuity ⇒ the invariant box is the whole space.** A "proof" that holds because
  the trapping region didn't actually trap anything (the empty-set-Mario failure)
  shows as a box coextensive with the axes — it doesn't *bite*.

These are the wish-language's characteristic bugs (spec, not execution), and each
has a geometric tell.

## III.4 The IDE is part of the language

Because comprehension is interrogation, the renderer is not a nice-to-have; it is
the **reading surface of the language**, the way a REPL is part of Lisp's surface.
The concrete view stack (extending the earlier prototype):

1. per-variable **range + sampled density** (interval via `Optimize` min/max;
   density via sampling) — "realistic range";
2. **pairwise joint region** for coupled pairs — the constraint, rendered;
3. the **constraint graph** — overview + module/claim boundaries;
4. **parallel coordinates** — all dimensions of the sampled cloud at once;
5. the **phase portrait / flow** — the transition over the cloud (≤3 projected
   dims): fixed points, cycles, sinks, the trapping region;
6. **trajectory + counterexample overlay** — a run as a path; a cex as a path
   escaping the box;
7. the **proven invariant region** drawn distinctly from **witnessed samples**
   (Part II.7's soundness honesty).

---

# Part IV — Worked examples

## IV.1 The bounded queue / cache daemon (we built this)

State: `q` ∈ ℤ, capacity `CAP`. Transition: `enq` (`q′=q+1`, guarded `q<CAP`),
`deq` (`q′=q−1`, guarded `q>0`), `idle`. One-dimensional portrait:

```
         enq         enq         enq         enq
      ┌───────►   ┌───────►   ┌───────►   ┌───────►
   [0]         [1]         [2]   …    [CAP-1]      [CAP]
      ◄───────┘   ◄───────┘   ◄───────┘   ◄───────┘
         deq         deq         deq         deq

   proven trapping region:  ╠════════════════════════╣  0 ≤ q ≤ CAP
                            (the flow never leaves it)
```

Spacer proves `0 ≤ q ≤ CAP` — the box around the whole line. Now **drop the
capacity guard** (the classic leak from `proven_cache.py`): an `enq` arrow now
leaves `CAP` to `CAP+1`, *escaping the box*. That escaping arrow is the
counterexample, and Spacer hands back exactly the trajectory `0→1→…→CAP→CAP+1`.
The bug is a single arrow crossing the wall.

## IV.2 The synthesized thermostat (C-style)

State: `temp` (and a *free* `on` the solver chooses). Hard band `LO ≤ temp ≤ HI`;
physics `temp′ = temp + GAIN·on − LOSS`; objective `minimize Σ on`. The portrait
is a 1-D `temp` axis with a *fan* at each point (two arrows: `on`, `off`); the
synthesizer's job is to pick, at each state, an arrow that keeps the next point
inside `[LO, HI]`. The **viability kernel** is the sub-band from which a safe
choice always exists; start outside it and *no* controller keeps you in band — the
portrait shows the fan with *both* arrows leaving the band, i.e. the daemon is
ill-posed there. This is "is the synthesis well-posed?" rendered.

## IV.3 A two-stage pipeline (convergence to a region)

State `(q0, q1)`. The flow spirals/settles into the difference-bounded region
`0 ≤ q0,q1 ≤ CAP` (an octagon-ish box) — which Spacer found in ms in the pipeline
experiment. Drawn, you *see* the reachable set expand from `(0,0)` and stop at the
octagon wall: the proof, animated.

---

# Part V — Honest limits

- **Dimensionality.** Beyond 3 state variables every view is a projection, and
  projections hide joint holes (II.8). The constraint graph mitigates but does not
  eliminate this; some structure is only visible in the right 2-D slice.
- **Set-valued flow is hard to draw.** A relation gives a *fan* of arrows per
  point; dense fans clutter. Aggregation (draw the post-image *region*, not every
  arrow) scales better but loses the per-choice detail.
- **Sampling bias and the soundness split (II.7).** Witnessed points are an
  under-approximation; proven regions are an over-approximation; conflating them
  lies in one of two directions. Near-uniform sampling is itself nontrivial.
- **Over-approximation gap.** The drawn invariant box may be strictly larger than
  the true reachable set (sound for safety, but it can suggest reachable states
  that aren't). Tightening costs a more expensive abstract domain (II.5).
- **Scale.** You never render the *global* portrait of a large daemon; you render
  per claim and rely on contracts to compose. If claims leak (no sealed boundary),
  this whole story degrades back to an unrenderable global blob — which is exactly
  why III.1's boundary discipline is load-bearing, not optional.

---

# References

**Dynamical systems & phase portraits (the intuition and the classical theory)**
- S. H. Strogatz, *Nonlinear Dynamics and Chaos*, 2nd ed., 2015. (The accessible
  canon for fixed points, limit cycles, basins, vector fields.)
- M. Hirsch, S. Smale, R. Devaney, *Differential Equations, Dynamical Systems, and
  an Introduction to Chaos*, 3rd ed., 2012.
- H. Poincaré, *Mémoire sur les courbes définies par une équation différentielle*,
  1881–1886. (Origin of the qualitative/phase-portrait viewpoint.)
- H. K. Khalil, *Nonlinear Systems*, 3rd ed., 2002. (Lyapunov stability, basins.)

**Set-valued dynamics, viability, controlled invariance (the synthesis/free-choice case)**
- J.-P. Aubin, A. Cellina, *Differential Inclusions*, 1984.
- J.-P. Aubin, *Viability Theory*, 1991. (Viability kernel = "stay-safe-forever" set.)
- C. Tomlin, J. Lygeros, S. Sastry, "A game-theoretic approach to controller design
  for hybrid systems," *Proc. IEEE*, 2000. (Controlled-invariant / winning regions.)

**Invariants, reachability, model checking (the proof = region machinery)**
- E. Clarke, O. Grumberg, D. Peled, *Model Checking*, 1999; C. Baier, J.-P. Katoen,
  *Principles of Model Checking*, 2008.
- A. Bradley, "SAT-Based Model Checking without Unrolling" (IC3), *VMCAI* 2011;
  N. Een, A. Mishchenko, R. Brayton, "Efficient implementation of property-directed
  reachability" (PDR), *FMCAD* 2011.
- A. Komuravelli, A. Gurfinkel, S. Chaki, "SMT-Based Model Checking for Recursive
  Programs" (**Spacer**), *CAV* 2014. (What our prototypes actually run.)
- A. Pnueli, R. Rosner, "On the synthesis of a reactive module," *POPL* 1989;
  R. Bloem et al., "Synthesis of reactive(1) designs" (**GR(1)**), 2012. (Write the
  spec, synthesize the transition.)

**Abstract interpretation & the shape/cost of the region (II.5)**
- P. Cousot, R. Cousot, "Abstract interpretation," *POPL* 1977.
- P. Cousot, N. Halbwachs, "Automatic discovery of linear restraints among variables
  of a program" (polyhedra), *POPL* 1978.
- A. Miné, "The octagon abstract domain," 2006. (Difference bounds — the cheap shape
  Spacer favors in our measurements.)
- A. Girard, "Reachability of uncertain linear systems using zonotopes," *HSCC* 2005.
- B. Jeannet, A. Miné, "Apron: a library of numerical abstract domains," *CAV* 2009.
- Reachability tools: G. Frehse et al., **SpaceEx** (*CAV* 2011); X. Chen, E. Ábrahám,
  S. Sankaranarayanan, **Flow*** (*CAV* 2013); M. Althoff, **CORA**; S. Gao, S. Kong,
  E. Clarke, **dReal/dReach**. I. Mitchell, "A toolbox of level-set methods"
  (Hamilton–Jacobi reachability), 2007.

**Abstraction & refinement (the quotient portrait, II.6)**
- S. Graf, H. Saïdi, "Construction of abstract state graphs with PVS," *CAV* 1997.
- E. Clarke, O. Grumberg, S. Jha, Y. Lu, H. Veith, "Counterexample-guided abstraction
  refinement" (**CEGAR**), *CAV* 2000; T. Ball, S. Rajamani, **SLAM**, 2002.

**Liveness ⇔ Lyapunov / ranking functions**
- A. Podelski, A. Rybalchenko, "A complete method for the synthesis of linear ranking
  functions," *VMCAI* 2004. (Ranking function = discrete Lyapunov = progress proof.)

**Sampling, counting, and uniformity (II.7)**
- S. Chakraborty, K. Meel, M. Vardi, "Balancing scalability and uniformity in SAT
  witness generation" (**UniGen**), 2014.
- R. Kannan, H. Narayanan, "Random walks on polytopes" (Dikin walk), 2009; R. Smith,
  "Hit-and-run," 1984.
- A. Barvinok, lattice-point counting; **LattE** (De Loera et al.). (Region volume /
  variable mass.)

**High-dimensional & constraint visualization**
- A. Inselberg, "The plane with parallel coordinates," *The Visual Computer*, 1985;
  *Parallel Coordinates*, 2009.

**Reactive/daemon surfaces that already got readability right (prior art for the surface)**
- M. Colledanchise, P. Ögren, *Behavior Trees in Robotics and AI: An Introduction*,
  2018. (A tree of intent, ticked each frame — the readable-daemon artifact, with
  the blackboard/cross-cutting-state lesson.)
- N. Halbwachs, P. Caspi, P. Raymond, D. Pilaud, "The synchronous data-flow
  programming language LUSTRE," *Proc. IEEE*, 1991; Kind2 (Champion et al., *CAV*
  2016) for its model checking.
- L. Lamport, *Specifying Systems* (TLA+), 2002. (Init + next-state action + temporal
  properties — the canonical transition-relation description.)

---

*Bottom line for the language design: build the claim around its phase portrait.
The six things a portrait needs (axes, box, shape, flow, trapping region, world-edges)
are the six things a claim's boundary should expose; the proof a user wants is the
region drawn around the flow; the bugs they fear are its topology; and the IDE that
draws it is not a tool beside the language but the language's reading surface.*

---

# Appendix — Direct manipulation: sculpting the portrait (speculative)

The body of this note treats the portrait as an *output* you read:
`constraints → solver → portrait`. The inverse is where the real power may be:
`portrait → manipulate → constraints` — the picture as the **authoring surface
itself**, edited like a vector-drawing program or ZBrush. This appendix sketches
the idea; it is speculative and unproven.

**The analogy is technical, not loose.** ZBrush ultimately edits an *implicit
surface* (a signed-distance-style field whose level-set is the shape), not
vertices. A feasible region is exactly that: **a constraint is an implicit
function, the region is its sub-level set.** So *constraints are to feasible regions
as SDFs are to sculpted shapes* — both implicit representations edited indirectly
and re-rendered. Drag a wall inward → stiffen a bound; carve a notch → add an
exclusion; crease an edge → a hard constraint; **paint a soft falloff → author an
objective** (`minimize distance-to-painted-region` — you paint where the solver
should prefer to be, gravity instead of a formula).

**The central hard problem is honest and classic: the inverse map is many-to-one.**
Dragging a boundary is satisfied by infinitely many constraint-edits
(`x ≤ 5`? `x + y ≤ 5`? a curve through the same point?). This is the
programming-by-demonstration ambiguity ("I nudged one corner and it rewrote my
model"). The answer is not a perfect guesser but three things together:

1. **A brush *vocabulary*, not freeform sculpting.** Each brush is a constraint
   *shape* with a unique reading — an axis-wall brush snaps to a bound on one
   variable; a diagonal brush gives a linear combination; an exclusion brush carves
   a region. The palette *is* the language's constraint forms, so every gesture is
   legible (cf. snapping/guides in vector tools).
2. **The solver in the loop as the disambiguator.** You needn't invert perfectly
   because you watch: sculpt → re-render → adjust (the refine loop of §III.3).
   Real-time consequence makes ambiguous edits safe.
3. **Lens laws as the editor's correctness spec.** Bidirectional programming already
   formalized "edit a view, propagate to the source": sculpting the region to what
   it already shows must not change the constraints (stability / GetPut); a pushed
   edit must then re-render as shown (PutGet). Those laws *define* a well-behaved
   sculpt tool.

The genuinely hard residue is that **editing a projection edits the whole** — you
sculpt a 2-D slice of an N-D region and the lift back is ambiguous (§II.8 in
reverse: "did you mean *always*, or only when `z` is in this range?"). The brush
must carry that intent or the system must ask.

**The endpoint that makes it more than a nicer editor:** because the *proof* is a
feature of the picture (§II.4), you can **draw the trapping region you want and have
the system synthesize the daemon that stays in it.** Sketch the box the flow must
never leave; synthesis produces the `_x/x` transition (the guards) that provably
traps it. That is reactive synthesis with a *drawn* specification instead of a
temporal-logic one — the phase portrait as the **spec surface**, sketched rather
than typed. Draw the safe region → get the controller, proven.

**Lineage / references.**
- I. Sutherland, *Sketchpad* (1963) — the original "draw it, add constraints, the
  system maintains them"; the patron saint of this whole idea.
- A. Borning, *ThingLab* (1979); G. Badros, A. Borning, P. Stuckey, **Cassowary**
  (2001) — constraint-drawing descendants (Cassowary underlies Apple Auto Layout).
- J. Foster, M. Greenwald, J. Moore, B. Pierce, A. Schmitt, "Combinators for
  bidirectional tree transformations" (lenses / **Boomerang**), *POPL* 2005 — the
  view-edit-to-source theory and the lens laws.
- S. Gulwani, "Automating string processing… by examples" (**FlashFill**),
  *POPL* 2011 — ranking ambiguous candidate programs (the PBE machinery to reuse).
- B. Shneiderman, "Direct manipulation," 1983; B. Victor, "Inventing on Principle,"
  2012; T. Schachman, *Recursive Drawing* / *Apparatus* — the direct-manipulation,
  immediate-feedback authoring philosophy and modern graphical constraint tools.

*If it works, this is not a side feature — it is the product, because "I sculpted
the behavior and the proof came with it" is something no current tool does.*
