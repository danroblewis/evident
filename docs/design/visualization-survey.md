# Visualizing programs and proofs — a survey, and where Evident's lens fits

> **Scope.** Evident draws a program *as a dynamical system* and a proof *as a
> geometric object*, backed by a solver. To know what's genuinely new in that and
> what we should borrow, this doc maps the broader space: how **programs** are
> visualized, how **provers / verification** are visualized, and where the
> dynamical-systems lens sits among them.
>
> **Companions (don't duplicate):** [`state-space-diagrams.md`](./state-space-diagrams.md)
> is our concrete diagram catalog (organized by the question each answers);
> [`phase-portraits.md`](./phase-portraits.md) is the thesis; the
> [`phase-portraits-research.md`](./phase-portraits-research.md) and
> [`morse-graphs.md`](./morse-graphs.md) notes are the verified dynamical-systems
> background. This doc is the *landscape map* around all of those.
>
> **Provenance.** Unlike `morse-graphs.md` (a verified research pass), this is a
> knowledge-based synthesis of established fields. Citations are to canonical works;
> any branch can be deep-researched on request to firm it up.

---

## The organizing idea

Every visualization here draws one of three **objects**, and the split is the
whole map:

1. **Structure** — the program's static form (syntax, control, dependence).
2. **Behavior** — what it does when it runs (executions, state, time).
3. **Proof** — *why* it's correct (derivations, reachable sets, invariants).

Evident's bet is that for a *constraint* program these collapse: the program *is*
its transition relation, behavior *is* the flow on a state space, and the proof
*is* a region of that space. So our work lives at the seam of "behavior" and
"proof," in a geometric register that most of the field keeps separate.

---

## Part 1 — Visualizing programs (software visualization)

The classic field of **software visualization / program comprehension** (Diehl,
*Software Visualization*, 2007) splits into **structure, behavior, evolution**.

### 1a. Static structure
| view | shows | field |
|---|---|---|
| **AST / syntax tree** | the parse structure | compilers, editors |
| **Control-flow graph (CFG)** | basic blocks + branches within a procedure | compilers, static analysis |
| **Call graph** | which function calls which | whole-program analysis |
| **Data-flow / def-use, Program Dependence Graph (PDG)** | how values flow; control+data dependence | slicing, optimization (Ferrante–Ottenstein–Warren 1987) |
| **Module / dependency / import graph** | architecture; coupling | reverse engineering |
| **UML class diagram** | types and relations | OO design |

These answer "*how is it built?*" They're the program's anatomy, independent of any
run.

### 1b. Dynamic behavior (a run)
| view | shows | field |
|---|---|---|
| **Execution trace / UML sequence diagram** | the order of calls/messages over time | debugging, distributed systems |
| **Flame graph / call tree** | where time/stack goes | profiling (Gregg) |
| **Heap / object / pointer diagram** | the runtime data graph; box-and-arrow memory | debuggers, teaching |
| **Algorithm animation** | data structure mutating step by step | pedagogy (BALSA, Brown 1988) |
| **Symbolic-execution tree** | the branching of *all* paths under symbolic inputs | test generation, bug finding (KLEE) |

These answer "*what happens when it runs?*" — one run (trace) or all runs
(symbolic tree).

### 1c. State machines — the bridge to behavior-as-structure
| view | shows | field |
|---|---|---|
| **State diagram / FSM** | states + labeled transitions | automata, protocols |
| **Statechart** (Harel 1987) | *hierarchical & parallel* states — nesting, orthogonal regions | reactive systems, UML |
| **Petri net** | concurrency, tokens, resource flow | concurrency theory |

Statecharts are the high-water mark of *drawing behavior as a structure*. This is
the discrete dual of our state-space view (`state-space-diagrams.md` §2): nodes are
whole state-vectors, edges are the transition relation made literal. Viable only
for **small finite** state — it enumerates, it doesn't sample.

---

## Part 2 — Visualizing proofs and verification

This is the "prover" half, and it splits by *what kind of proof*.

### 2a. Logical proof objects (the syntactic tradition)
| view | shows | field |
|---|---|---|
| **Derivation / proof tree** | a proof as a tree of inference-rule applications (natural deduction, sequent calculus) | proof theory |
| **Proof-assistant state** | current **goals + hypotheses**; the proof *script* structure; tactic state | Coq, Lean, Isabelle, Agda |
| **Proof DAG (resolution)** | how an UNSAT result was derived; the resolution/DRAT chain | SAT solving |
| **Implication graph / conflict analysis** | CDCL decision levels + learned clauses | modern SAT (Chaff, MiniSat) |
| **Unsat core** | the minimal contradictory subset | SMT debugging |

These render the **logical structure of a proof**. Note Evident does *not* live
here — we produce *geometric* certificates, not derivation trees. (A real gap to be
honest about: §5.)

### 2b. Model checking (the state-space tradition)
| view | shows | field |
|---|---|---|
| **Kripke structure / reachability graph** | the explored state space | explicit-state MC (SPIN) |
| **Counterexample trace** | the path to a bad state — often a **lasso** (stem + cycle) for liveness | LTL/CTL model checking |
| **BMC unrolling** | the transition relation unrolled k steps, handed to SAT/SMT | bounded model checking |
| **Reachable-set frontier** | the reachable set growing tick-by-tick toward/into a safe region | symbolic MC |
| **CEGAR loop** | predicate-abstraction graph + refinement when a spurious counterexample appears | software MC (SLAM, BLAST) |

This is the family Evident is closest to operationally — *enumerate-or-symbolically-
explore the state space* — but model checking draws it as a **graph**, where we draw
it as a **flow / region** (Baier & Katoen, *Principles of Model Checking*, 2008).

### 2c. Abstract interpretation — invariants AS geometry (our closest classical relative)
Abstract interpretation (Cousot & Cousot 1977) computes an over-approximation of
reachable states in an **abstract domain**, and the standard domains *are shapes in
variable space*:

| domain | shape | source |
|---|---|---|
| **Intervals** | axis-aligned boxes | Cousot–Cousot 1977 |
| **Zones / Octagons** | `±xᵢ ±xⱼ ≤ c` — octagons | Miné 2006 |
| **Polyhedra** | convex polyhedra (linear invariants) | Cousot–Halbwachs 1978 |
| **Ellipsoids / templates** | quadratic / template regions | Sankaranarayanan et al. |

The computed invariant is **literally a region drawn in state space** — exactly our
"trapping region" overlay. The **lattice** of abstract values, and **widening /
narrowing** (how the analysis converges), are themselves visualized. *This is the
field whose pictures look most like ours* — the difference is they get the region by
fixpoint iteration in a fixed abstract domain; we get it by solving.

### 2d. Certificates as geometric objects (control + computational dynamics)
| certificate | drawn as | field |
|---|---|---|
| **Inductive invariant** | the region the system can't leave | verification |
| **Lyapunov / ranking function** | a height the flow descends → stability / termination | control theory; termination provers |
| **Barrier certificate** | a surface the trajectory can't cross | control (Prajna 2004) |
| **Reachable-set flowpipe** | the tube of reachable states over time | hybrid-systems reachability (Flow*, SpaceEx, CORA) |
| **Conley–Morse graph** | the recurrence skeleton + per-set Conley index — a *computer-assisted proof* of global structure | computational dynamics (CMGDB/DSGRN) — see [`morse-graphs.md`](./morse-graphs.md) |

This row is the heart of where Evident is headed: **the proof and the picture are
the same object** (the thesis), and the dynamical-systems community already does
this rigorously (Conley–Morse), as does hybrid-systems control (flowpipes,
Lyapunov, barriers).

---

## Part 3 — The dynamical-systems lens, and Evident's position

The lens we've adopted — *program = dynamical system; state space + transition;
proof = geometry* — is a real synthesis of four traditions:

- **Nonlinear dynamics** (Strogatz): phase portraits, fixed-point classification,
  the conservative/dissipative honesty rule. → `phase-portraits-research.md`.
- **Computational dynamics** (Conley–Morse, CMGDB/DSGRN): the rigorous global
  skeleton via outer-approximated transition graphs. → `morse-graphs.md`.
- **Abstract interpretation**: invariants as regions in a domain.
- **Hybrid-systems / control**: reachable sets, Lyapunov, barriers, flowpipes.

**What is distinctive about *our* position** (the parts not already standard):

1. **The transition is *queryable by a solver*.** Classical tools build the
   over-approximating map with bespoke **interval arithmetic**; we get the
   set-valued image as a **Z3 query** (`pin _state ∈ box, solve for the enclosure`).
   That is the rigorous outer approximation Morse/abstract-interpretation need,
   *for free and provably sound* — stronger than the one published "queryable
   model" precedent, which is only probabilistic (`morse-graphs.md` §3).
2. **Program and dynamical system are the *same object*.** In a constraint
   language there's no modeling gap: the `fsm` body *is* `f`. The phase portrait is
   not a model *of* the program — it's a rendering of the program.
3. **Three views, one object.** Phase portrait (flow) + trapping region (safety
   proof) + Morse graph (recurrence skeleton) are projections of a single
   solver-backed transition relation — the unification `state-space-diagrams.md`
   organizes and `phase-portraits.md` argues for.
4. **Discrete state is exact.** For finite enum/bool state there's no
   approximation — the transition graph and its Morse decomposition are exact
   (`morse-graphs.md` §5), which the continuous-grid literature doesn't address.

**What we borrow, from whom:**

| borrow | from |
|---|---|
| fixed-point classification, conservative/dissipative honesty, cobweb, nullclines | nonlinear dynamics |
| outer-approximation → SCC → Morse graph → Conley index | computational dynamics (CMGDB) |
| invariant-as-region, the lattice/widening view | abstract interpretation |
| counterexample-as-trajectory, reachable-frontier-as-proof, the lasso | model checking |
| Lyapunov height, barrier surface, flowpipe tube | control / hybrid systems |
| statecharts, the full-state-vector graph (small finite) | software viz / `swap` |

---

## Part 4 — The consolidated map

Pick a view by **what object** and **what question**:

| object | question | view(s) | our status |
|---|---|---|---|
| structure | how is it built? | CFG, call/dependence graph, AST | n/a (not our focus) |
| behavior (1 run) | what did this run do? | time series, timing diagram, trace | **have** / demo |
| behavior (all runs) | how does state flow? | **phase portrait**, streamlines, cobweb | **have** / partial |
| behavior (finite) | exact discrete structure? | state-transition graph, statechart | demo |
| behavior (recurrence) | what are the attractors/cycles? | **Morse graph** (SCC condensation) | **planned** (`morse-graphs.md`) |
| proof (logical) | why, as a derivation? | proof tree, tactic state, unsat core | **gap** (§5) |
| proof (reachability) | can it reach bad? | reachability graph, counterexample, BMC frontier | overlay: **have**/future |
| proof (invariant) | what region traps it? | invariant region, abstract domain | **have** (overlay) |
| proof (stability/liveness) | does it settle / progress? | Lyapunov contours, ranking surface, Morse order | **have** (energy) / planned |
| parameters | when does it break? | bifurcation, parameter phase diagram | demo |
| high-D | too many variables? | scatterplot matrix, parallel coords, projections | **have** / partial |

(Full per-diagram detail: `state-space-diagrams.md`.)

---

## Part 5 — Honest gaps and where we're deliberately different

- **We don't draw logical proof structure** (Part 2a). Our certificates are
  *geometric* (regions, Morse graphs), not derivation trees or tactic states. That's
  a deliberate stance — "the proof is the picture" means the *geometric* proof — but
  it means a whole half of the prover-visualization world (proof assistants, SAT/SMT
  derivations) is outside our frame. Worth knowing we've chosen a side.
- **High dimensionality** is the shared wall (everyone projects, and projections
  lie — `phase-portraits-research.md` §3.3). No tradition has solved it; we inherit
  the limit.
- **Interactivity / direct manipulation** (sculpting the portrait to author the
  program — `phase-portraits.md` appendix) is speculative everywhere; almost no tool
  closes the edit↔picture loop, which is where the language ambition actually is.
- **The conservative/dissipative honesty rule** (don't draw a flow field for an
  area-preserving map) is known in dynamics but routinely violated by naive
  plotters — including ours, until we fix it. A differentiator if we get it right.

---

## References

- S. Diehl, *Software Visualization*, Springer, 2007 — the structure/behavior/
  evolution taxonomy.
- D. Harel, "Statecharts: a visual formalism for complex systems," *Sci. Comput.
  Program.* 8(3), 1987.
- J. Ferrante, K. Ottenstein, J. Warren, "The program dependence graph," *ACM
  TOPLAS* 9(3), 1987.
- C. Baier, J.-P. Katoen, *Principles of Model Checking*, MIT Press, 2008 —
  reachability, counterexamples, BMC, the enumerate view.
- E. Clarke et al., "Counterexample-guided abstraction refinement" (CEGAR), CAV
  2000.
- P. Cousot, R. Cousot, "Abstract interpretation," POPL 1977; P. Cousot, N.
  Halbwachs, "Automatic discovery of linear restraints among variables," POPL 1978;
  A. Miné, "The octagon abstract domain," *HOSC* 19, 2006.
- S. Prajna, A. Jadbabaie, "Safety verification using barrier certificates," HSCC
  2004; reachability tools **Flow\*** (Chen–Ábrahám–Sankaranarayanan),
  **SpaceEx** (Frehse et al.), **CORA** (Althoff).
- S. H. Strogatz, *Nonlinear Dynamics and Chaos*, 2015.
- The Conley–Morse computational line (Kalies, Ban, Mischaikow, Mrozek, Arai,
  Gedeon, Pilarczyk; CMGDB, DSGRN) — full citations in
  [`morse-graphs.md`](./morse-graphs.md).
- A. Inselberg, *Parallel Coordinates*, Springer, 2009.
- *swap* state-space tool — the full-state-vector graph: https://www.youtube.com/watch?v=YGLNyHd2w10
