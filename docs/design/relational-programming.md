# Relational Programming and Evident

## What is relational programming?

A **function** from A to B is a special kind of relation — one that is *functional*
(each input gives exactly one output) and *total* (defined for every input).
A **relation** is just a subset of A × B with no such restrictions.

Relational programming treats programs as relations rather than functions.
The same program can check a solution, generate solutions, or enumerate all valid
pairs — because there is no privileged "input" or "output" direction.

Evident is a constraint programming language in the relational tradition.
`claim sorted xs ys` doesn't say "xs maps to ys." It says "xs and ys stand in the
sorted relationship." Fix either one; solve for the other.

---

## The landscape

| Language | Style | Execution | Order matters? | Constraint solver | Evidence |
|---|---|---|---|---|---|
| Prolog | Logic/relational | SLD resolution, depth-first | Yes (cuts, ordering) | Unification only | No |
| miniKanren | Relational | Interleaving search | No | Unification only | No |
| Datalog | Datalog/relational | Bottom-up fixpoint | No | None (pure derivation) | No |
| CLP(FD) | Logic + constraints | Prolog + propagation | Yes (Prolog layer) | Finite domains | No |
| MiniZinc | Constraint modeling | Compiled to solver | No | Full CP/SMT | No |
| ASP (Clingo) | Answer sets | SAT-based | No | SAT + constraints | No |
| SQL | Relational algebra | Query planning | No | None | No |
| **Evident** | Constraint/relational | Fixpoint + solver | No | Full CP/SMT | **Yes** |

---

## Prolog — relational in syntax, procedural in practice

Prolog was the first practical logic programming language. Programs are Horn clauses;
variables are logical variables; execution is SLD resolution with depth-first search.

```prolog
sorted([]).
sorted([_]).
sorted([A, B | Rest]) :- A =< B, sorted([B | Rest]).
```

The problem: clause ordering is semantically significant. `ancestor(X, Z) :- ancestor(X, Y), parent(Y, Z)` loops forever; swap the goals and it works. Programmers must hold the execution model in mind constantly. Cut (`!`) commits to a branch and destroys bidirectionality. Real Prolog programs are full of procedural control.

Kowalski acknowledged this in 1979: "Algorithm = Logic + Control." Prolog conflates what to compute (logic) with how to search (control). Evident separates them: the programmer writes only the logic; the solver owns control.

**What Evident takes from Prolog:** Horn clause semantics, unification as variable identification, the basic claim/evidence structure.

**What Evident drops:** execution ordering, cut, negation-as-failure as the default, procedural control.

---

## miniKanren — relational programming made explicit

miniKanren (Byrd & Friedman, 2005) is the most explicit statement of the relational
programming philosophy. Everything is a relation. Programs are written so they can run
in any direction.

```scheme
(defrel (appendo l s out)
  (conde
    [(== l '()) (== s out)]
    [(fresh (a d res)
       (== l (cons a d))
       (== out (cons a res))
       (appendo d s res))]))

;; Forward: append two lists
(run* (out) (appendo '(1 2) '(3 4) out))   ; → ((1 2 3 4))

;; Backward: split a list
(run* (l) (appendo l '(3 4) '(1 2 3 4)))   ; → ((1 2))

;; Generate: all splits
(run* (l s) (appendo l s '(1 2 3 4)))       ; → (() (1 2 3 4)) ((1) (2 3 4)) ...
```

The same `appendo` relation checks, generates, and enumerates. No separate
"split" function needed.

miniKanren uses interleaving search (not depth-first), making it complete —
it will eventually find all answers. The constraint solver is unification only;
arithmetic and other constraints are handled by extensions (cKanren).

**What Evident takes from miniKanren:** the relational philosophy, bidirectionality,
variables as unknowns rather than names for values, no privileged input/output.

**What Evident adds:** a real constraint solver (arithmetic, set operations, etc.),
types as constraints, evidence terms, claim composition via variable identification.

**Key difference:** miniKanren uses search. Evident uses constraint propagation.
Propagation is not complete (some things require search) but is much faster for
the common case of well-constrained problems.

---

## Datalog — relational without functions

Datalog is Prolog restricted to no function symbols. This makes it decidable and
fully declarative — rule ordering is irrelevant, bottom-up evaluation is complete,
the same minimal model is always produced.

```datalog
reachable(X, Y) :- edge(X, Y).
reachable(X, Z) :- edge(X, Y), reachable(Y, Z).
```

Every rule is a Horn clause with no function symbols. Evaluation is a fixpoint
computation: apply all rules until no new facts are derivable.

Datalog is used extensively for program analysis (Doop, Soufflé) and deductive
databases. It is order-independent and parallelizable. Its weakness: no arithmetic,
no compound data structures, no open-ended computation.

**What Evident takes from Datalog:** the fixpoint semantics, order-independence,
the minimal model. Evident's execution model is Datalog extended with a constraint
solver for arithmetic and set operations.

**What Evident adds:** arithmetic, set operations, compound types, evidence terms,
the full expressive power of constraint programming.

---

## CLP(FD) — Prolog + real constraint solving

Constraint Logic Programming over Finite Domains adds a real finite-domain
constraint solver to Prolog. Posting `X #> 0` constrains X's domain immediately,
before search begins. Arc consistency propagates constraints.

```prolog
n_queens(N, Queens) :-
    length(Queens, N),
    Queens ins 1..N,
    all_different(Queens),
    ...
    label(Queens).
```

The constraint `all_different(Queens)` uses an efficient propagator (Régin's
bipartite matching algorithm) rather than naive backtracking. Domains are narrowed
by propagation before search explores residual freedom.

CLP(FD) demonstrated that adding a real solver transforms Prolog's practical
usefulness. But Prolog's ordering and procedural control remain — the logic and
control are still conflated.

**What Evident takes from CLP(FD):** the recognition that constraint propagation
is essential, not optional. The solver as a first-class participant.

**What Evident drops:** Prolog's ordering, the `label/1` step as a programmer
concern, the procedural layer.

---

## MiniZinc — constraint modeling without programming

MiniZinc is a constraint modeling language: you describe the problem,
not the solution procedure. The model is compiled to a solver backend.

```minizinc
var 1..n: queen_row;
constraint all_different(queens);
solve satisfy;
```

MiniZinc is closer to Evident than any other language: purely declarative,
no functions, the solver handles everything. Its model/solver separation is
exactly what Evident aspires to.

**What Evident takes from MiniZinc:** the model-not-program philosophy,
global constraints as named library primitives, the solver as the execution engine.

**What Evident adds:** a relational/logic programming layer, evidence terms,
claims as named constraint systems, types as constraints, the set-theoretic model,
composable sub-systems via variable identification.

**Key difference:** MiniZinc programs describe a static constraint model.
Evident programs are constraint systems that compose, inherit from each other,
and carry first-class evidence. Evident is also a programming language, not
just a modeling notation.

---

## Answer Set Programming — stable model semantics

ASP (Clingo, DLV) computes stable models — consistent sets of beliefs under
a closed-world assumption. No execution order. Classical negation supported.

```asp
link(a, b). link(b, c). link(c, d).
reachable(X, Y) :- link(X, Y).
reachable(X, Z) :- link(X, Y), reachable(Y, Z).
```

ASP is fully order-independent. The computational engine is SAT-solving
with constraint propagation (DPLL/CDCL). ASP handles classical negation
(`-p` = p is explicitly false) alongside negation-as-failure (`not p`).

**What Evident takes from ASP:** order-independence, classical negation,
the SAT-based solver infrastructure.

**Key difference:** ASP is designed for combinatorial search problems.
Evident is a general-purpose programming language. ASP has no type system;
Evident's types are constraints.

---

## What Evident adds to the tradition

Every language above makes some version of the relational insight — programs
should describe relationships, not procedures. Evident's specific contributions:

**1. Evidence as first-class values.** When a claim is established, the derivation
tree is a value you can inspect, pass around, and reason about. No existing
relational language treats evidence this way.

**2. Claims define sets.** `claim sorted xs ys` names the set of pairs where ys
is a sorted permutation of xs. Constraint accumulation is set intersection. The
programmer's job is to write claims whose solution space is exactly what they want.

**3. Types are constraints.** `T ∈ Ordered` is the same kind of statement as
`n ∈ Nat` — both are set-membership constraints. No separate type system is needed.
Dependent types emerge naturally from the constraint model.

**4. Data structures from graph primitives.** A list is a linked list is a constrained
graph. No primitive list type required — the sequential structure emerges from
`graph → dag → tree → linked_list` each adding one constraint.

**5. Composable constraint systems.** Claims are mixed in via variable identification
(`..claim`), not inheritance or method dispatch. Shared variables flow automatically.
The solver handles the composition.

**6. No ordering, ever.** Evident takes this further than any predecessor: not just
"ordering doesn't affect results" (Datalog, MiniZinc) but "ordering is not a concept
in the language." Claim bodies are simultaneous constraints. The solver finds any
valid order.

---

## The core insight

Every programming language is ultimately about describing what is true.
Functional languages describe it as transformations. Object-oriented languages
describe it as state and behavior. Logic languages describe it as provable facts.

Relational languages — and Evident specifically — describe it as **membership in sets**.
A claim is a set. A constraint is a membership condition. The solver finds witnesses.

The shift from "write a program" to "describe a constraint system" is the same shift
that happened in database queries (SQL), constraint optimization (MiniZinc), and
formal verification (Alloy, TLA+). Evident applies it to general-purpose programming.
