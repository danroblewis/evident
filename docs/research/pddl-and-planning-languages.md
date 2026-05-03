# PDDL and AI Planning Languages

> Research for Evident language design — understanding the AI planning tradition,
> what it shares with Evident's constraint-based approach, and where the two worlds
> genuinely differ.

---

## Executive Summary

PDDL (Planning Domain Definition Language) is the standard language for classical AI planning,
developed in 1998 for the International Planning Competition and now in its 3.x generation.
It is relevant to Evident because the problems are structurally similar — both express constraints
on states and transitions — but the execution models are fundamentally different. PDDL feeds a
**search engine** that finds action sequences; Evident feeds an **SMT solver** that finds
satisfying assignments. That distinction drives every practical difference between them.

There is a third tradition — *planning as satisfiability* — that bridges the two. SMTPlan
(Cashmore et al., 2016) compiles PDDL+ problems into SMT formulas and solves them with Z3.
Evident is doing something adjacent, but without the PDDL front-end and with a more
constraint-native modeling style. Understanding the planning literature helps the team know
what Evident is reinventing, what it genuinely offers that planners don't, and what problems
the planning community has already solved that Evident will eventually face.

---

## PDDL Structure and Syntax

PDDL splits every problem into two files:

**Domain** — the reusable part (shared across problem instances):
```pddl
(define (domain logistics)
  (:requirements :strips :typing :equality)
  (:types location vehicle)
  (:predicates
    (at ?v - vehicle ?l - location)
    (road ?from - location ?to - location))
  (:action drive
    :parameters (?v - vehicle ?from - location ?to - location)
    :precondition (and (at ?v ?from) (road ?from ?to))
    :effect (and (not (at ?v ?from)) (at ?v ?to))))
```

**Problem** — a specific instance:
```pddl
(define (problem deliver-1)
  (:domain logistics)
  (:objects depot city - location  truck - vehicle)
  (:init (at truck depot) (road depot city))
  (:goal (at truck city)))
```

The domain defines what *can* happen. The problem defines what *is* and what *should be*.
A planner finds a sequence of action instances that transforms `:init` into `:goal`.

**PDDL 2.1** added numeric fluents (mutable numeric state variables) and durative actions
with temporal qualifiers — `at start`, `at end`, `over all`:

```pddl
(:durative-action move
  :parameters (?r - rover ?from ?to - waypoint)
  :duration (= ?duration (distance ?from ?to))
  :condition (and (at start (at ?r ?from))
                  (over all (passable ?from ?to)))
  :effect (and (at start (not (at ?r ?from)))
               (at end (at ?r ?to))
               (at end (decrease (battery ?r) (* ?duration 2)))))
```

**PDDL 3.0** adds trajectory constraints — temporal logic over the full plan trace,
expressed as soft and hard preferences. A plan can satisfy `:goal` while violating
preferences, with a quality penalty. This is the planning community's answer to
"not all valid plans are equally good."

---

## Related Formalisms

**STRIPS** (1971) — the original. Binary predicates, closed-world assumption, no disjunction
in preconditions, no conditional effects. PDDL is STRIPS with 30 years of extensions.

**ADL** (Action Description Language, Pednault 1987) — added conditional effects (`when`
clauses), open-world assumption, and disjunctive preconditions. Most of ADL is absorbed
into PDDL 2.x.

**HTN** (Hierarchical Task Networks) — instead of search from initial state to goal,
decompose high-level tasks into subtasks recursively. More expressive than STRIPS-based
planning (technically undecidable in the general case), closer to procedural programming.
The `HDDL` language attempts to standardize HTN in the PDDL ecosystem. HTN planners know
*how* to achieve goals, not just *what* the goal is.

**RDDL** (Relational Dynamic influence Diagram Language, Sanner 2010) — the language for
the probabilistic planning track of the IPC. Semantically a factored MDP expressed as a
dynamic Bayesian network. Handles stochastic transitions and continuous variables.
Syntactically inspired by PDDL but conceptually a different beast: every transition is a
conditional probability distribution, not a deterministic state update.

**TLA+** (Temporal Logic of Actions, Lamport) — not a planning language but deeply related.
A behavior is `Init ∧ □[Next]_vars`: an initial condition plus an action that relates
the current state to the next. TLA+ is for specification and model checking, not plan
generation. The `Next` action looks exactly like an Evident transition schema:
`state ∈ GameState, next ∈ GameState` with constraints relating them. The key difference:
TLA+ checks whether *all* behaviors satisfy a property; Evident *finds* behaviors that satisfy
a property.

**ANML** (Action Notation Modeling Language, Smith et al.) and **NDDL** (NASA's New Domain
Definition Language, used by the EUROPA planner) — higher-level alternatives to PDDL,
designed for richer temporal and resource modeling. ANML supports both generative and HTN
modes. These exist because PDDL's Lisp-style syntax and limited type system are genuinely
painful for large domains.

---

## Planning as Satisfiability: Where Evident Lives

The "planning as satisfiability" tradition is the intellectual ancestor of Evident's approach
to planning-like problems. Kautz and Selman (1992) showed that STRIPS planning can be encoded
as propositional SAT: introduce Boolean variables for each (action, time-step) pair, add
clauses for preconditions, effects, and frame axioms, and ask the SAT solver whether the
formula is satisfiable for a given plan horizon. Increment the horizon until satisfiable
(this is bounded model checking).

**SMTPlan** (Cashmore, Fox, Long, Magazzeni, 2016) extends this to PDDL+, the full hybrid
planning language with continuous numeric change. It encodes the planning problem as an
SMT formula over Z3, with real-valued state variables at each time step, Boolean action
variables, and constraints for preconditions, effects, frame axioms, and temporal
relationships. Z3 either finds a satisfying assignment — which decodes to a valid plan —
or reports UNSAT (no plan of that length exists).

This is structurally what Evident does for transition schemas like `GameTransition`. The
differences:

1. **Encoding vs. native modeling.** SMTPlan translates from PDDL to SMT as a compilation
   step. Evident's source language *is* the constraint language — no translation layer,
   but also no PDDL compatibility.

2. **Horizon management.** SMTPlan searches over plan lengths: try horizon 1, 2, 3... until
   SAT. Evident's streaming executor doesn't do this — it solves one transition at a time,
   stepping forward. It cannot currently find a multi-step plan that reaches a goal without
   explicit encoding of the plan as a fixed-length sequence of transition variables.

3. **Frame axioms.** In SAT/SMT planning, frame axioms (facts not mentioned in an effect
   stay unchanged) are explicit formula constraints. In Evident's current model, there is
   no automatic frame: if you don't constrain `next.health`, the solver may change it
   arbitrarily. This is a real limitation for planning-style use.

4. **Goal reachability vs. goal satisfaction.** SMTPlan checks reachability: can we reach
   a state satisfying `:goal`? Evident can *verify* that a state satisfies a schema, but
   cannot currently do bounded reachability search without user-written iteration.

**Bottom line:** Evident is not reinventing SMTPlan — it is building a more general
constraint modeling language that *can be used* for planning-like problems, but without the
planning-specific infrastructure (bounded horizon search, automatic frame axioms,
goal-directed search).

---

## What Evident Can Learn from Planning Languages

**The domain/problem split is genuinely useful.** PDDL's separation of reusable domain
logic from specific problem instances is a form of parameterization that Evident handles
through schema arguments, but there is no conventional distinction. In Evident, the
"domain" and "problem" are just schemas — which is flexible but makes it harder to swap
problem instances against a fixed domain definition. A naming convention or module system
could formalize this.

**Frame axioms need a story.** In any transition-based model, the question "what changes
and what doesn't?" is unavoidable. PDDL's closed-world assumption (only listed effects
change) is restrictive but safe. Evident's open world (anything unconstrained can vary)
can produce surprising solver behavior. A `frame` keyword or convention saying "all
unmentioned fields of `next` equal the corresponding fields of `state`" would make
transition schemas much less error-prone.

**Plan quality and optimization.** PDDL 3.0 preferences and `:metric` allow expressing
plan quality explicitly. Evident already has `minimizing`/`maximizing` in query syntax —
this is the right direction, but documenting the analogy would help users understand what
they're getting.

**Benchmark problems.** The planning community has decades of benchmark domains (logistics,
blocks world, scheduling, route planning) with known solution properties. These are
excellent test cases for any constraint-based system that claims to handle transition
problems. Translating even two or three PDDL benchmark domains into Evident would stress-test
the language and reveal gaps.

**Negative effects and assertion retraction.** PDDL explicitly models `(not (at ?v ?from))`
as an effect — a predicate that was true becomes false. Evident transitions that model this
must encode it as `next.at_from = false` or equivalent. There is no shorthand. The planning
literature has extensively studied how to make this ergonomic.

---

## Where Evident Offers Something Planning Languages Don't

**Bidirectional solving.** A PDDL planner finds a plan to achieve a goal from a given initial
state. Evident can solve in any direction: given constraints on the output, find a satisfying
input. Given a desired `next` state, find a `state` and `cmd` that transition into it. This
has no standard equivalent in PDDL and is the most distinctive capability Evident brings.

**Constraint programming as modeling style.** PDDL actions are operational: they say what
happens when you execute them. Evident constraints are declarative: they say what is true of
a valid transition without prescribing how to find it. This means Evident can express
underspecified transitions (the solver chooses any valid response) naturally, where PDDL
would require explicit nondeterminism extensions.

**Uniform schema language.** In PDDL, goals, initial states, preconditions, and effects
all have different syntactic forms. In Evident, world definitions, transition rules, and
goal conditions are all schemas with the same membership-condition syntax. A `ValidFinalState`
schema is just another schema — it can be queried, composed, and extended without special
treatment.

**Richer type system inline.** PDDL's type hierarchy is nominal and flat (`:types location
vehicle`). Evident's types are defined by constraint schemas, making structural subtyping
natural. A `Task` that satisfies `ScheduledTask` is a subtype by constraint inclusion,
not by declaration.

**No planner required.** The flip side of Evident's limitation on multi-step planning is
that it does not need a planner at all for single-step problems. The Z3 backend handles
scheduling, validation, type checking, and game transition in a uniform framework.
Planning-specific heuristics (Fast Downward's causal graphs, FF's relaxed plan heuristic)
are irrelevant to Evident's use cases, which are constraint satisfaction, not long-horizon
plan search.

---

## What It Would Take to Express a PDDL Domain in Evident

A minimal translation of a STRIPS domain would require:

1. **Types → Evident types** (`type Location = { id ∈ Nat }`) — straightforward.
2. **Predicates → Boolean fields in a state schema** — PDDL's `(at ?v ?l)` becomes a
   relational constraint like `v.location = l.id` in the state.
3. **Actions → transition schemas** — each PDDL action becomes a schema over
   `(state, next, action_params)` with preconditions as constraints on `state` and
   effects as constraints on `next`.
4. **Frame axioms → explicit field equalities** — every field not mentioned in an action's
   effect must be copied: `next.battery = state.battery`. Currently manual.
5. **Goal → query** — `? FinalState s` where `FinalState` names the goal schema.
6. **Planning horizon → user-written iteration** — not currently native.

Items 4 and 6 are the real gaps. A `transition` keyword that automatically frames
unmentioned fields, and a `reachable_in n steps` operator, would make Evident a first-class
planning language without abandoning its constraint-native identity.

---

## Summary Judgments

| Question | Answer |
|---|---|
| Is Evident reinventing SMTPlan? | No — SMTPlan is a compiler from PDDL to SMT; Evident is a native constraint language. Overlap in mechanism, not in purpose. |
| Can Evident express PDDL domains? | Yes for single-step; multi-step planning requires manual horizon encoding and frame axioms. |
| Does Evident have PDDL's ecosystem? | No. No benchmark library, no planner comparison, no IPC participation. |
| What's Evident's most novel capability vs. planning? | Bidirectional solving — find inputs from output constraints. No PDDL planner does this. |
| What's Evident's biggest gap vs. planning? | Frame axioms and bounded reachability search. Both are solvable with targeted language additions. |
