# Phase portraits — what the field actually does (research notes)

> Companion to [`phase-portraits.md`](./phase-portraits.md). That doc is *our*
> thesis ("a daemon is a dynamical system; the proof is the picture"). This doc is
> the **standard textbook/practice treatment** of phase portraits, gathered to
> check what a good one contains and where our `evident phase-portrait` tool
> (`runtime/src/viz.rs`) is thin. Sourced from Strogatz *Nonlinear Dynamics and
> Chaos*, Hirsch–Smale–Devaney, MIT 18.03, Scholarpedia/Wikipedia/Encyclopedia of
> Mathematics, and university dynamical-systems notes (citations at the end).

---

## 1. What a phase portrait is

For a system `ẋ = f(x,y)`, `ẏ = g(x,y)`:

- **Phase space / phase plane** — the set of all states; each state is a single
  **point**; the axes are the *state variables*. **Time is not an axis** (1-D =
  phase line, 2-D = phase plane, n-D = phase space).
- **Vector / direction field** — the RHS assigns a velocity `(ẋ, ẏ)` to every
  point. A *direction field* normalizes those to unit length (orientation only).
- **Trajectory / orbit** — a solution curve, everywhere **tangent** to the field.
- **The portrait** — a representative set of trajectories with arrowheads for time
  direction. Its power: the **qualitative long-term behavior of many initial
  conditions at once, without solving the ODE.** Two portraits "mean the same" iff
  a homeomorphism maps orbits→orbits preserving time direction (**topological
  equivalence**) — so a portrait is about *shape and direction*, not exact numbers.

## 2. The anatomy of a good portrait (built in this order)

A good portrait is the **qualitative skeleton**, not "as many trajectories as
possible." The construction order is itself the lesson:

### 2.1 Fixed points (equilibria) — the anchors
`f(x*) = g(x*) = 0` (both rates vanish). Found, in the standard method, as the
**intersections of the nullclines** (§2.3).

### 2.2 Their classification — the information-dense heart
Linearize: take the **Jacobian** `A` at the fixed point, read its eigenvalues.
With trace `τ = λ₁+λ₂`, determinant `Δ = λ₁λ₂`, discriminant `τ²−4Δ`, three
switches decide everything: **saddle vs not** = sign of Δ; **node vs spiral** =
sign of τ²−4Δ; **stable vs unstable** = sign of τ.

| Type | Eigenvalues | Looks like |
|---|---|---|
| **Saddle** | real, opposite signs (Δ<0) | in along stable eigenvector, out along unstable; always unstable |
| **Stable node (sink)** | real, both <0 | straight-line inflow, tangent to the slow eigenvector |
| **Unstable node (source)** | real, both >0 | straight-line outflow |
| **Stable spiral/focus** | complex, Re<0 | spirals inward |
| **Unstable spiral/focus** | complex, Re>0 | spirals outward |
| **Center** | pure imaginary (τ=0, Δ>0) | concentric closed loops; neutrally stable, *not* asymptotically |
| **Star (proper node)** | repeated real, 2 eigenvectors | radial straight rays |
| **Degenerate (improper) node** | repeated real, 1 eigenvector | all tangent to the single eigenvector |

**The trace–determinant plane** packs all of this into one picture: plot each
system at `(τ, Δ)`. Parabola `τ²=4Δ` divides real (nodes/saddles, below) from
complex (spirals/centers, above); `Δ<0` is saddles; the τ-sign splits
stable/unstable. **Asymptotic stability ⇔ τ<0 and Δ>0.**

Two caveats the texts stress:
- **Star vs degenerate node can't be told from τ,Δ alone** — count eigenvectors.
- **A linearized *center* is fragile**: on the imaginary axis Hartman–Grobman
  fails, so nonlinear terms can turn a predicted center into a slow spiral.
  Centers are robust only with extra structure (a conserved quantity, reversibility).

### 2.3 Nullclines — the scaffold you draw first
- **x-nullcline**: `ẋ = f = 0` → on it the flow is **vertical** (only y moves).
- **y-nullcline**: `ẏ = g = 0` → flow is **horizontal**.
- **Intersections of the two different families = the fixed points.**
- Each nullcline separates `ẋ>0` from `ẋ<0` (resp. y), tiling the plane into
  regions of constant flow-sign — one representative arrow per region sketches the
  whole qualitative flow with *no integration*. (An **isocline** `f=c` is the
  general "constant slope" curve; the nullcline is the `c=0` case.)
- Canonical example: **Lotka–Volterra** — horizontal prey-nullcline, vertical
  predator-nullcline, crossing at the coexistence equilibrium (a center).

### 2.4 The global skeleton
- **Separatrices = saddle stable/unstable manifolds = basin boundaries.** A
  saddle's **stable manifold** is the canonical separatrix: an *invariant* wall
  (trajectories can't cross it) dividing the plane into different fates. Draw these
  *first* to organize everything; seed orbits on **both sides** of each.
- **Limit cycles** — *isolated* closed orbits (stable / unstable / semi-stable).
  Distinct from a center's continuous *family* of closed orbits.
- **Poincaré–Bendixson**: in 2-D the only long-term behaviors are fixed points and
  limit cycles — **planar chaos is impossible** (fails in ≥3-D).
- **Basins of attraction** — the set of starts converging to each attractor; their
  boundaries are usually the saddle separatrices, and **can be fractal**
  (Grebogi–Ott–Yorke), giving sensitive dependence of *final state* on start.
- **Homoclinic / heteroclinic orbits** — connections saddle→itself / saddle→other;
  the wiring behind bifurcations and chaos.
- **Invariant sets** are the unifying abstraction: equilibria, manifolds,
  separatrices, limit cycles are all invariant.

### 2.5 Conventions and pitfalls
- **Filled dot = stable, open circle = unstable** (half-filled = 1-D semistable; a
  saddle is open + its manifolds drawn). Bifurcation diagrams: solid line = stable
  branch, dashed = unstable.
- **Arrowheads = increasing time.** **Trajectories never cross** (uniqueness) —
  a crossing in a drawing is an error.
- Seed **one orbit per nullcline/separatrix-carved region**; the separatrices are
  the most important orbits to draw.
- Pitfalls: omitting separatrices (asserts one fate where two exist); too few
  orbits (misses structure); too many (clutter); a **regular grid of arrows**
  (aliasing/moiré — use jittered/evenly-spaced seeding); undersampling the field
  (misses critical points); trusting a linearized center. A 2005 user study
  (Laidlaw et al.) found **locating critical points and identifying their type** is
  the most method-sensitive task — exactly the analytical content.

---

## 3. Two distinctions that change the picture — and that we got wrong

### 3.1 Continuous flow vs. discrete map
Textbook portraits are for ODEs (smooth flow). **An Evident FSM tick is a *map*:
`xₙ₊₁ = f(xₙ)`.** Consequences:
- The canonical 1-D map portrait is the **cobweb / staircase diagram**: plot
  `y=f(x)` and the diagonal `y=x`, then bounce (vertical to the curve = apply f,
  horizontal to the diagonal = feed output back as input). Fixed points sit at
  curve∩diagonal; **stability is the slope** there: `|f'(x*)|<1` stable, with
  monotone (0<f'<1) vs oscillatory (−1<f'<0) convergence; period-2 = a box loop;
  chaos fills a region.
- **Map stability is the UNIT CIRCLE `|λ|<1`**, *not* the ODE left-half-plane
  `Re(λ)<0`. (They're linked by `z=e^{λΔt}`, which maps the LHP to the unit disk.)
  Using "negative real part" on a map is the wrong test.
- A map's orbit is a **sequence of discrete dots** — the state *teleports*
  `xₙ→xₙ₊₁` with nothing between, so you **cannot connect iterates with a smooth
  flow line**. Regular orbits trace out invariant curves; chaos fills a region
  (the Hénon/standard-map "islands in a chaotic sea").
- A **k-cycle** is a fixed point of `fᵏ`; its multiplier is the **product**
  `∏ f'(xᵢ)` along the cycle (eigenvalues of `∏ Df` in n-D).
- **Bifurcation / orbit diagrams** (attractor vs a parameter — period-doubling
  cascade, Feigenbaum δ≈4.669) are the complementary "across parameters" view.

### 3.2 Conservative vs. dissipative — the honesty rule (the big one)
- **Dissipative** (volume-contracting, `|det J|<1`): *attractors exist*;
  trajectories genuinely funnel onto them. **A direction field is honest.**
- **Conservative / area-preserving** (`|det J|=1`, symplectic): **no attractors
  are possible** — volume preservation + **Poincaré recurrence** forbid the
  contraction an attractor needs. Orbits are recurrent/closed and **never merge**.
  A normalized direction field then **implies a convergence that cannot exist** —
  at best redundant, at worst a lie (sharpest case: the area-preserving hyperbolic
  **Arnold cat map**, where field arrows along eigendirections falsely suggest gray
  orbits flowing into a highlighted one).
- **The honest view for a conservative system is the orbits as LEVEL SETS of the
  conserved quantity** `E`. Since `dE/dt = 0`, the orbits *are* the contours of
  `E` — **contour-plot `E` and you have the exact phase portrait, no integration.
  The proof (E constant on orbits) is literally the picture.** Canonical: the
  frictionless **pendulum**, `E = ½θ̇² − cos θ`; closed contours = libration, wavy
  contours = rotation, **separatrix at `E=1`** through the unstable fixed point at
  `θ=π`. Add damping and `E` decreases monotonically — trajectories then **cross**
  every level set and spiral to an attractor, and you need the vector field again.
  **An attractor's existence is itself proof that no conserved quantity exists.**

This is exactly our companion doc's "the proof is the picture," made precise: it
holds *exactly* for conservative/integrable systems.

### 3.3 High-dimensional state — the honest limits
Past ~3 variables every view is partial:
- **2-D projection** onto a chosen variable pair is standard, but **a projection
  can show apparent crossings that aren't real** (uniqueness holds only in full
  space), and **joint structure collapses** — each variable can be in range while
  the *joint* state is forbidden (the marginal problem). *Disjoint projections ⇒
  disjoint originals; overlapping projections do NOT ⇒ overlapping originals.*
- **Poincaré section / return map** — slice with a surface transverse to the flow;
  records successive crossings, reducing an N-D flow to an (N−1)-D **map**.
  Periodic orbit → fixed point(s); quasiperiodic → closed curve of points; chaos →
  scattered cloud. Limit-cycle stability = eigenvalues of the return map's Jacobian
  inside the unit circle (**Floquet multipliers**).
- **Parallel coordinates** keep all axes but **lose time ordering** of a trajectory.
- **PCA / t-SNE / UMAP / delay embedding (Takens)** preserve dynamical *invariants*
  (dimensions, Lyapunov exponents) but **not geometry/coordinates**, each with
  distortion and parameter costs.

---

## 4. Scorecard — `evident phase-portrait` vs. the standard

| Standard element | Status in our tool |
|---|---|
| Vector field + trajectories | ✓ (trajectories drawn as dots — correct for a map) |
| **Fixed-point classification** (node/saddle/spiral/center) | ✗ — biggest gap; we report no *type* |
| Stability test (unit circle `|λ|<1` for maps) | ✗ — no stability computed |
| Fixed points by **solving** `f(x)=x` | ✗ — found only by walking into self-loops |
| **Nullclines** | ✗ |
| **Separatrices / basin boundaries** | ✗ — and we don't seed on both sides |
| Cobweb (canonical 1-D map portrait) | ✗ — we use a state-line (fine, not standard) |
| Conservative→level-sets vs dissipative→arrows | ✗ — **arrows drawn indiscriminately; misleading for Lotka & undamped pendulum** |
| Stability markers (filled/open) | ✗ |
| Poincaré sections / high-D handling | ✗ (less urgent) |

Our four numeric samples, judged by §3.2: **spring** (dissipative spiral sink) and
**van der Pol** (limit cycle) — arrow field honest ✓. **Lotka–Volterra** (center)
and **undamped pendulum** (conservative) — should be drawn as **orbits / level
sets**, not an arrow field.

## 5. The opportunity — we have a solver

Most of these gaps are hand-or-symbolic steps in a normal tool, but our query
primitive (`step(x)` = solve the transition) makes the **analytical** versions
cheap:

- **Fixed points** — one query: solve `state = _state` (finds *all* of them,
  reachable or not; numeric coordinates included).
- **Classification** — compute the **Jacobian by finite differences** from
  transition queries (`f(x+εeᵢ) − f(x)`), take eigenvalues → node/saddle/spiral/
  center, with the correct **map** stability test (`|λ|` vs 1). *Highest-value
  addition.*
- **Conservative vs dissipative** — the same Jacobian's **determinant** (`|det|`
  vs 1) decides whether to draw arrows or switch to **orbits/level-sets**.
  Automatic honesty.
- **Conserved quantity / invariant** — the deep version: find the level sets
  directly, or synthesize the invariant region (the companion doc's Spacer route).

Bottom line: we built a competent *flow plotter* but skipped what makes a phase
portrait *analytical* — equilibrium classification and the conservative/dissipative
honesty switch — and both are in reach precisely because we can solve the transition.

---

## Sources

- **Texts**: Strogatz, *Nonlinear Dynamics and Chaos* (Ch. 5–8; §6.5 conservative
  systems, §6.7 pendulum, §8.7 Poincaré maps, Ch. 10 1-D maps); Hirsch–Smale–
  Devaney; Guckenheimer & Holmes Ch. 1.
- **Course notes**: MIT 18.03 / 18.03SC (trace–determinant, isoclines), MIT 12.006J
  (Poincaré/Hénon), UBC Math 215, Berkeley MCB137 (nullcline sketching), UC Davis
  *Discrete Dynamical Systems: Maps*, Lamar/Paul's Online Notes, Chalmers
  (Strogatz lecture notes), UChicago REU & Southampton (Poincaré–Bendixson).
- **Reference**: Wikipedia (*Phase portrait*, *Phase plane*, *Cobweb plot*,
  *Limit cycle*, *Stable manifold theorem*, *Arnold's cat map*, *Standard map*,
  *Hénon map*, *Poincaré map*, *Bifurcation diagram*, *Logistic map*, *Takens's
  theorem*, *Parallel coordinates*); Scholarpedia (*Basin of attraction*,
  *Equilibrium*, *Chirikov standard map*); MathWorld (*Area-Preserving Map*,
  *Feigenbaum Constant*); Encyclopedia of Mathematics (*Separatrix*, *Saddle*).
- **Primary / visualization**: Grebogi–Ott–Yorke (fractal basin boundaries);
  Alexander–Yorke–You–Kan (riddled basins); Sauer–Yorke–Casdagli (*Embedology*);
  Laidlaw et al. 2005 (vector-vis user study); Inselberg 1985 (parallel coordinates).

*Caveat carried from the research: Strogatz/HSD full text wasn't openly fetchable;
their statements here are cross-corroborated across the secondary sources above,
which agreed on every load-bearing point.*
