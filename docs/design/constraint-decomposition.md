# Constraint Decomposition: Surfacing Structure the Programmer Already Wrote

## The Core Observation

When a programmer writes a schema, they write one unified set of constraints.
They don't think about separate automata or sub-problems. But the constraints
themselves encode a structure: two variables are coupled if they appear together
in any constraint. Variables that never share a constraint are independent —
they have no relationship, and their parts of the schema could be solved
without any awareness of each other.

This structure is already there, implicit in what the programmer wrote. The
question is whether to surface it.

---

## What the Structure Is

Build the **constraint hypergraph** from a schema:
- Each variable is a node
- Each constraint is a hyperedge connecting all variables it mentions

The connected components of this hypergraph are fully independent sub-problems.
No constraint in component A references any variable in component B. They have
no relationship by definition.

In constraint automata terms: the product decomposition A = A₁ ▷◁ A₂ ▷◁ … ▷◁ Aₙ
exists naturally, without the programmer writing it. Each connected component
is one factor.

The **tree-width** of the constraint hypergraph is a formal measure of how
coupled the system is. Lower tree-width means the components are more
independent. Higher tree-width means more entanglement. This is an existing
concept in constraint theory with known complexity implications.

---

## What Happens When You Add a Constraint

Every time a new constraint is added, the runtime can check:
- Does this constraint mention variables from one existing component? → No
  structural change; it joins that component.
- Does this constraint mention variables from two or more existing components?
  → Those components merge. The schema becomes less decomposable.

This is the useful signal. The programmer made two previously independent
parts of their schema dependent on each other. Maybe that was intentional —
they genuinely need that relationship. Maybe it was accidental — they reached
for a shared variable without realizing it coupled two otherwise-separate
concerns.

---

## The Programmer Insight, Not the Optimization

The primary value is not making the program run faster. It is making the
program's structure visible.

A constraint schema can grow large. Variables accumulate. Constraints
proliferate. The programmer loses track of what depends on what. The
decomposition analysis is a structural mirror: it shows the programmer the
actual dependency topology of what they wrote, independent of any execution
concern.

This is analogous to what a type system does: it doesn't change what the
program computes, it surfaces structure that was already implicit in the code
and flags when that structure is violated or surprising. Or what a linter's
circular dependency check does: it doesn't prevent the code from running, it
tells the programmer that two modules are more entangled than they perhaps
intended.

For constraint systems specifically, the decomposition tells the programmer:
- Which variables are truly independent (never share a constraint)
- Which variables are tightly coupled (appear together in many constraints)
- Which constraints are the "bridge" constraints that couple otherwise-separate
  concerns
- How the structure of the schema changes as they add each constraint

The IDE visualization could be as simple as: colored groupings of variables,
where a new constraint that merges two groups is highlighted. "This constraint
joins the position cluster and the velocity cluster."

---

## The Side Effect: Parallel Execution

If the connected components are genuinely independent, the runtime can execute
them as separate constraint automata running concurrently. Each component is
solved without needing to know about the others. Results are combined at the
end (or never combined, if they have no shared ports).

This is beneficial. But it's the side effect, not the goal. The goal is
programmer understanding. The parallelism falls out of the same analysis that
produces the structural feedback.

The reason to do this before significant optimization work: the decomposition
is not an optimization. It's just reading the structure of what the programmer
wrote. It requires no clever algorithms — a union-find over the constraint
hypergraph is sufficient. The cost is low and the programmer feedback is
immediate.

---

## What Changes and What Doesn't

**Nothing changes about how schemas are written.** The programmer writes one
unified schema. No new syntax for declaring separate components. No manual
partitioning.

**The runtime analyzes the schema** after parsing and builds the dependency
graph. It reports the components and their sizes. If a constraint merges two
previously separate components, that's noteworthy.

**The IDE can visualize the components.** Each component gets a color or
grouping. Constraints that are "bridges" between components are highlighted.
The tree-width or component count can be displayed as a structural metric.

**The execution can exploit the decomposition.** Independent components are
solved separately. This is an implementation detail — the programmer doesn't
configure it, it just happens.

---

## Connection to the Automata Model

In the constraint automata framework: a schema with N connected components
is naturally the product of N separate constraint automata. Variables shared
between components (if any) become the ports connecting them — the seams in
the product construction.

The decomposition analysis is the inverse of product construction: given A,
find A₁, A₂, …, Aₙ such that A = A₁ ▷◁ A₂ ▷◁ … ▷◁ Aₙ. The constraint
hypergraph connected components give this decomposition directly.

Variables that appear in constraints from multiple components are the shared
ports. The runtime synchronizes on those ports when executing the decomposed
automata.

---

## What This Requires to Build

1. **Constraint hypergraph construction**: after parsing a schema, traverse all
   constraints and record which variables each mentions. One pass, O(constraints
   × variables per constraint).

2. **Connected component detection**: union-find over the hypergraph. Near-linear
   time.

3. **Change detection**: when a constraint is added (in the IDE's live parse
   loop), check if it merges components. Simple set intersection check.

4. **Reporting**: surface the component structure to the programmer. Can be as
   simple as logging, as rich as IDE visualization.

5. **Parallel execution**: route each component to a separate solver call. The
   existing `rt.query()` infrastructure handles one component at a time; calling
   it in parallel for independent components is straightforward.

None of this is novel algorithmically. The value is in applying it to constraint
schemas and surfacing it to the programmer.

---

## Open Question

What is the right threshold for "this merge is worth telling the programmer
about"? Not every constraint that touches two variables needs a notification.
The signal is most useful when it merges large, coherent components — when the
programmer can recognize the two clusters as distinct concerns that they
perhaps hadn't intended to couple. Heuristics for this (component size,
constraint specificity, naming patterns) are worth exploring.
