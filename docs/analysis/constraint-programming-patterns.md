# A Gang-of-Four for Constraint Programming

### Design patterns discovered in the Evident self-hosted compiler (`compiler2/`)

> Status: exploratory analysis. This document reads `compiler2/` as a
> *corpus* — one of the first large programs written in Evident — to
> discover the recurring design patterns of constraint programming. It
> changes no source. File:line references are to the tree at branch
> `cp-design-patterns`.

---

## 1. Thesis

The "Gang of Four" cataloged the recurring shapes of object-oriented
construction: not algorithms, but *the structural and behavioral
arrangements that keep showing up* once you build large systems out of
objects and messages. This document asks the analogous question for a
different substrate: **what are the recurring shapes once you build a
large system out of constraints and a solver?**

Evident is an unusually sharp lens for this question because of its
execution model, which is worth stating precisely:

- A program is a set of **constraints** over named variables.
- Execution is a sequence of **ticks**. Each tick, a Z3 SMT solver
  finds a satisfying assignment for the whole constraint set, the
  kernel reads back the `effects` Seq from the model and dispatches it
  (I/O, libffi calls, exit), and the results are fed back as
  `last_results` on the next tick.
- The only memory across ticks is the **carry**: a top-level variable
  `x` of primitive type is re-asserted next tick as `_x = <its model
  value>`. So `_x` is "x one tick ago."

This is **FSM-over-SMT**: a deterministic finite-state machine whose
transition function is expressed *entirely as constraints*, re-solved
from scratch every tick. There is no mutable store, no loop, no call
stack, no random-access array in the language — only constraints, the
one-tick carry, and pattern-match over recursive enums. And yet
`compiler2/` is a ~7,800-line program that lexes, Pratt-parses, and
emits SMT-LIB for a substantial subset of Evident, *by building Z3 ASTs
in memory via libffi*.

A program that ambitious, written under those constraints, is forced to
**reinvent — in constraint form — most of the machinery a normal
language gives you for free**: a stack, a heap, a symbol table, a
program counter, subroutine calls, an instruction interpreter, a
sliding parse window. The shapes it uses to do so are the patterns this
catalog names. They are *not* the patterns of the classical CP modeling
literature (see §2): those are about how to encode a combinatorial
*search* problem so a solver finds the answer efficiently. These are
about how to drive a *deterministic computation* over a solver that
re-solves every tick — closer in spirit to GoF (how to *construct*
software) than to MiniZinc's handbook (how to *model* a puzzle).

The catalog is organized into four GoF-style categories:

- **Structural** — how data and types are organized in/around the model.
- **Behavioral** — how constraints drive computation across ticks.
- **Creational** — how Z3 objects and effects get built.
- **Compositional** — how claims combine (Evident-specific; the closest
  analog to GoF's "object vs. class" axis).

---

## 2. Prior art (web survey)

There is a real, if young, literature on **constraint-programming
patterns**, but it sits at a different altitude than this catalog.

- **Domain-specific constraint patterns** — the most explicit "patterns
  for CP" effort. A live repository ([constraintpatterns.com]) and the
  backing paper (Kelareva et al., *Easy, adaptable and high-quality
  Modelling with domain-specific Constraint Patterns*, [arXiv:2206.02479])
  use a recognizably GoF-like template — *recurring problem → solution
  approach (expert/best-practice modelling) → consequences* — but each
  pattern is tied to a problem *domain* (scheduling, timetabling,
  TSP/permutation) and bundles the global constraints and search
  strategies known for that domain. These are patterns for *encoding a
  search problem*, not for *driving a computation*.
- **MiniZinc "Effective Modelling Practices"** ([MiniZinc Handbook §2.7])
  is the canonical idiom list: tight **variable bounds**, **dual /
  viewpoint models** with **channeling constraints**, **redundant
  (implied) constraints**, and **symmetry breaking**. All are about
  making *search* converge — again, a different concern.
- **SMT-LIB encoding idioms** — **reification** (associate a 0/1 var
  with a constraint's truth), the **element constraint** (array indexing
  via an index variable), and **constraint decomposition** into
  auxiliary variables ([Effective encodings of CP models to SMT],
  St Andrews; [An Encoding for CLP Problems in SMT-LIB], arXiv:2404.14924).
- **Answer Set Programming** contributes two micro-idioms that *do* show
  up here in spirit: the **fact** (rule with empty body) and the
  **integrity constraint** (rule with empty head — "this must not
  happen"), plus the pervasive **generate-and-test** structure.

**The gap.** None of this prior art catalogs patterns for using a
constraint solver as a *general-purpose computational substrate driven
by a state machine*. The classical work treats the solver as an oracle
you call once to solve a static puzzle; Evident calls it 100,000 times,
threading state through it to run an interpreter. The closest single
overlap is **reification** (Evident's whole "lower a boolean expression
to a Z3 handle" path is reification taken to its logical extreme — see
*Reify-to-Handle*, §5.5). The classic CP modeling patterns
(channeling, symmetry breaking, redundant constraints) are essentially
**absent** from compiler2, precisely because compiler2 does no search:
every tick's constraint set is engineered to have exactly one model.
That absence is itself the finding — **FSM-over-SMT is a genuinely
under-cataloged regime**, and the patterns below appear to be new
contributions rather than restatements.

Sources are listed in full in Appendix B.

---
