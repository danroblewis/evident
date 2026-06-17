# State-space & phase-space diagrams: a catalog

> Companion to [`phase-portraits.md`](./phase-portraits.md). The phase portrait is
> one lens on a constraint model's state space; this is the broader toolbox.
> Organized by the **question each diagram answers**, because that — not the
> drawing technique — is how you pick one. For every entry: what it shows, when to
> reach for it, what it needs (continuous/discrete, dimensionality, *cardinality*),
> and status (**have** = built, **demo** = demonstrated in `prototype/`,
> **future** = catalogued only).

A constraint model is a state space + a (possibly relational) transition. Every
diagram below is a projection of that one object onto some axes — *state* axes,
*time*, *parameters*, or *frequency*. Knowing which axes a diagram uses tells you
what it can and can't reveal.

---

## 1. "How does state flow?" — dynamics in state space (phase-space family)

| diagram | shows | needs | status |
|---|---|---|---|
| **Phase portrait** (direction field + trajectories) | where every state goes next; fixed points, cycles, trapping regions | ≤2–3 plotted dims; samples Int/Real fine | **have** |
| **Streamlines** | continuous flow lines (smoother than arrows) | continuous, 2-D | **have** |
| **Nullclines** | curves where one component is stationary (`Δx=0`); their crossings ARE the fixed points | 2-D, components separable | future |
| **Basin of attraction map** | colour each start state by which attractor/halt-set it reaches — the partition of state space | bounded 2-D slice; sampled | **demo** |
| **Poincaré section / return map** | sample the trajectory each time it crosses a surface → reveals periodicity vs chaos | recurrent/oscillatory | future |
| **Cobweb diagram** | iterate a 1-D map `x'=f(x)` against the diagonal — convergence / oscillation / chaos at a glance | 1-D recurrence | future |

The phase portrait and its kin are the *only* family that handles Int/Real
state directly (by sampling), because they never enumerate states — they sample
points and ask the solver for successors.

## 2. "What's the discrete structure?" — transition graphs (the *swap* family)

Nodes = whole state vectors, edges = transitions. The literal picture of the
transition relation. **Only viable for low-cardinality, finite state** — you draw
*every* reachable state vector, so it excludes Int/Real (infinite) and blows up
combinatorially. For small bounded enums/ints it's the clearest possible view,
and it's what the *swap* state-space tool builds
([video](https://www.youtube.com/watch?v=YGLNyHd2w10)).

| diagram | shows | needs | status |
|---|---|---|---|
| **State-transition graph** | the full reachable-state-vector graph; the transition relation made literal | small finite state (≤ ~hundreds of states) | **demo** |
| **Reachability tree** | BFS unrolling from init; the shortest path to each state (and to a bad state = counterexample) | finite/bounded | future |
| **Quotient / predicate-abstraction graph** | collapse concrete states by a few predicates → a small abstract graph; scales to ANY dimension by changing what a "node" means | any (you choose the predicates) | future |
| **SCC condensation** | strongly-connected components = the cycles; condensing them exposes liveness (a cycle with no progress = livelock) | finite | future |
| **Statechart** (Harel) | hierarchical/parallel state machines — nested states, orthogonal regions | structured finite | future |

This family is the discrete dual of the phase portrait: same transition relation,
but *enumerated* (graph) instead of *sampled* (flow). Use it when the state is a
handful of small-domain variables; use the phase portrait when it's numeric.

## 3. "How does it behave over time?" — time-domain (timing family)

The axis is **tick**, not state. Collapses an N-D trajectory to readable tracks.

| diagram | shows | needs | status |
|---|---|---|---|
| **Time series** (state vars / a scalar vs tick) | a run's trajectory; total occupancy vs the proved envelope | any; one run | **have** |
| **Timing diagram** (EE waveforms) | each boolean/enum signal as a track, value over time, transitions as edges — the digital-logic view | boolean/enum signals (+ a few analog) | **demo** |
| **Event / effect trace** (Gantt-ish) | emitted effects and their durations along time — the *commit* view | effectful runs | future |

The "metrics dashboard" (§ reduce, in phase-portraits.md) lives here: a daemon's
high-D state monitored as a few scalars over time. Timing diagrams are the same
idea for *digital* state — exactly the shape EE uses, and the natural view for a
mode/flag-heavy daemon (the thermostat's heater, a protocol's phase).

## 4. "How does input map to output?" — I/O & frequency (control family)

The axis is **frequency** (or an input signal). These characterise a system by its
*response*, not its internal state. **Caveat: classical transfer functions assume a
linear time-invariant (LTI) system.** A constraint daemon is generally nonlinear
and discrete, so these apply only after **linearising around an operating point**,
or as an *empirical* response to a chosen input stream.

| diagram | shows | needs | status |
|---|---|---|---|
| **Transfer function** `H(s)=out/in` | the I/O map in the Laplace/frequency domain | LTI (or local linearisation) | future |
| **Bode plot** | gain & phase vs frequency | LTI | future |
| **Nyquist plot** | `H(jω)` traced in the complex plane → closed-loop stability | LTI | future |
| **Step / impulse response** | output vs time to a unit step/impulse — works for *nonlinear* systems too (empirical) | any; an input | future |
| **Root locus** | how the poles move as a gain varies → the stability boundary | LTI, parametric | future |

The user's instinct that "a phase portrait kind of *is* a transfer function" is
half right: both characterise dynamics, but a phase portrait is **state→state**
(internal, full nonlinear geometry) while a transfer function is
**input→output** (external, linear summary). They are complementary projections —
one keeps the state and drops the I/O boundary, the other keeps the I/O boundary
and drops the state. For an effectful daemon the **step response** (empirical, any
nonlinearity) is the reachable bridge; the rest want linearisation.

## 5. "How does behaviour change with parameters?" — parameter space

The axis is a **parameter**, not state or time. This is the *phase diagram* in the
thermodynamics sense (regimes), and the link to **bifurcation** we drew for
parametric claims (a safe daemon turning unsafe at a critical parameter).

| diagram | shows | needs | status |
|---|---|---|---|
| **Bifurcation diagram** | sweep a parameter, plot the long-run state(s) — fixed point → cycle → chaos cascade | a parametric 1-D-ish recurrence | **demo** |
| **Parameter phase diagram** | colour parameter space by *regime* — e.g. safe/unsafe, or "a controller exists" (viability) | parametric; per-point analysis | future |
| **Root locus** | pole movement vs gain (see §4) | LTI parametric | future |

A daemon's safety boundary in parameter space *is* a phase diagram; its crossings
*are* bifurcations. "At what CAP / gain does my daemon stop being safe?" is a
parameter-space question, answered here, not in the phase portrait.

## 6. "Where does it live / how is it distributed?" — density & structure

| diagram | shows | needs | status |
|---|---|---|---|
| **Reachable-set / occupancy heatmap** | where the system spends time (density over state) | sampled run | future |
| **Per-variable marginal / histogram** | the *realistic range* of each variable | sampled | future |
| **Parallel coordinates** | all dims of the sampled set at once (Inselberg) | high-D sampled | future |
| **Scatterplot matrix** (pairwise projections) | every 2-D projection — the high-D escape | any dim | **have** |
| **Constraint / coupling graph** | variables as nodes, an edge where a constraint couples them → the module/decomposition structure | any | future |

This family answers "what's feasible / where's the mass," not "how does it move."
The scatterplot matrix (the pipeline projections) and the constraint graph (read
off the *inert* projections) are the high-dimensional workhorses.

## 7. "Is it correct?" — verification overlays

Not standalone diagrams — overlays that turn any of the above into a proof view.

| overlay | shows | status |
|---|---|---|
| **Invariant / trapping region** | the proven box the flow can't leave | **have** |
| **Counterexample trace** | a path escaping the invariant (the leak) | **have** |
| **Lyapunov / ranking surface** | a height the flow descends — stability/progress, as contours | **have** (energy contours) |
| **Reachable-set growth** (BMC frontier) | the reachable set expanding tick-by-tick into the invariant — the safety proof, animated | future |

---

## Choosing one: the cheat-sheet

- **Numeric state, want the dynamics** → phase portrait (+ overlays). Samples Int/Real.
- **A few small-domain variables, want the exact structure** → state-transition graph (swap-style). Finite only.
- **Watch one run / monitor a daemon** → time series; for flags/modes, a timing diagram.
- **Characterise I/O response** → step response (any), or transfer-function/Bode (linearise first).
- **"What happens as I change a parameter?"** → bifurcation / parameter phase diagram.
- **Too many dimensions** → scatterplot matrix + constraint graph (project), or reduce to a scalar time series, or abstract to a quotient graph. (See phase-portraits.md §"too many dimensions.")
- **Prove it** → overlay the invariant region / counterexample on whichever view fits.

The unifying point: pick the diagram by the **axes the question lives on** —
state, time, parameter, or frequency — and by whether the state is **numeric
(sample it)** or **small-and-finite (enumerate it)**.

---

## References

- S. H. Strogatz, *Nonlinear Dynamics and Chaos*, 2015 — phase portraits, nullclines,
  basins, Poincaré sections, cobweb, bifurcation (the whole §1 + §5).
- D. Harel, "Statecharts: a visual formalism for complex systems," *Sci. Comput.
  Program.*, 1987 — hierarchical state-transition diagrams.
- C. Baier, J.-P. Katoen, *Principles of Model Checking*, 2008 — reachability
  graphs, SCC/liveness, the enumerate-don't-sample view.
- K. Ogata, *Modern Control Engineering* (or Åström & Murray, *Feedback Systems*) —
  transfer functions, Bode, Nyquist, root locus, step response.
- D. Harris, S. Harris, *Digital Design and Computer Architecture*, 2012 — timing
  diagrams / digital waveforms.
- A. Inselberg, *Parallel Coordinates*, 2009 — high-dimensional set visualization.
- *swap* (YouTube) state-space tool — the full state-vector graph, the inspiration
  for §2: https://www.youtube.com/watch?v=YGLNyHd2w10
