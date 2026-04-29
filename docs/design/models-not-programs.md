# Models, Not Programs: Evident and Constraint Solving

## The Core Insight

Evident is not a programming language in the traditional sense. It is a **modeling language** where the algorithm is implied by the structure of the model, and a constraint solver fills in wherever the model is sufficiently specified.

This framing resolves the tension that has run through all of our design discussions. When Evident's examples looked like functional programs, it was because we were writing algorithms in claim notation — imposing an execution order on what should be a declarative model. The right mental model is not "write a program that computes X" but "describe what it means for X to be true, and let the runtime find out whether it is."

The programmer's job, stated precisely: **provide enough structure that the constraint solver can determine claims without exhaustive search**. The more structure you provide, the less time the solver spends. Perfect programming is a model so well-structured that the solver never branches — it reaches answers by propagation alone. Good-enough programming is structure that makes search tractable. Bad programming is a model so underspecified that the solver cannot terminate.

This reframes what it means to "write a program" in Evident. You are not specifying computation. You are constraining a solution space until the solver can find its way through it.

---

## What Prolog Got Wrong

Prolog's execution engine is unification plus depth-first backtracking. Unification is the most naïve possible constraint solver: it only knows about structural term equality. It has no concept of a variable's *domain*, no constraint propagation, no arc consistency, no learned clauses. Every constraint check is a runtime test. Every failure is local and learns nothing.

The N-Queens problem shows the gap concretely. In Prolog, placing a queen at (1,1) does nothing to restrict where later queens can go. The solver generates full placements and checks for conflict only after the fact. For N=8, it explores thousands of doomed configurations.

CLP(FD) — constraint logic programming over finite domains — adds a real finite-domain solver. Placing a queen at (1,1) immediately *removes row 1 and both diagonals from the domain of every future column*. The solver does not generate those configurations. Propagation eliminates them before they exist. The search tree shrinks by orders of magnitude.

And CLP(FD) is still one of the weaker solver architectures available. A full SMT solver (Z3, CVC5) adds linear arithmetic, equality with uninterpreted functions, algebraic data types, bitvectors, arrays, and strings — all integrated through the Nelson-Oppen combination framework. CDCL SAT solving underneath adds clause learning: when a contradiction is reached, the solver records a *learned clause* that prevents it from ever making the same combination of assignments again. Non-chronological backtracking jumps back to the earliest decision that caused the conflict, not just the last one.

The gap from Prolog's unification to a real SMT solver is not incremental. It is architectural.

---

## The "Sufficiently Bound" Threshold

A claim is **sufficiently bound** when the constraint problem it poses is decidable and tractable for the solver. Below this threshold, the solver can dispatch the claim without any further programmer-specified decomposition. Above it, the programmer must provide more structure.

What makes a claim sufficiently bound?

**Decidable theories** that modern solvers handle completely:
- Linear integer and real arithmetic: `a + b <= c`, `x > 0` — solver decides instantly
- Equality with uninterpreted functions: structural equality, congruence closure
- Algebraic data types: list membership, constructor matching
- Bitvectors: machine arithmetic with overflow
- Arrays: indexed access with read/write consistency

**What pushes outside the tractable zone:**
- Nonlinear arithmetic: `x * y = z` with all three unbound — undecidable over integers in general
- Universal quantification over infinite domains: `∀x. P(x)` where x is unbounded
- Heap mutation and aliasing — notoriously hard to encode

The self-evident leaves of the claim hierarchy are exactly the claims the solver can discharge without further decomposition. `a <= b` is self-evident not because it is a tautology, but because the arithmetic solver checks it directly. `card_number_passes_luhn(n)` is self-evident because there is a deterministic algorithm the runtime can run. `valid_schedule(jobs, assignments)` is *not* self-evident — it requires decomposition until the components are sufficiently constrained.

The programmer's art is: decompose high-level claims until each piece is in a theory the solver handles. Stop decomposing when you've crossed the threshold.

---

## Tell, Ask, and the Evidence Base

The cleanest formal model for Evident's runtime comes from Saraswat's Concurrent Constraint Programming (CCP, 1989), which identifies two primitive operations on a shared constraint store:

- **Tell** `c`: add constraint `c` to the store. The store grows monotonically — tell never removes information.
- **Ask** `c`: check whether the store *entails* `c`. Succeed immediately if yes; suspend (or fail) otherwise.

This maps precisely onto Evident:

- `assert fact` is **tell**: add a ground fact to the evidence base
- `? claim` is **ask**: check whether the claim is entailed by what's been established

The evidence base *is* a constraint store. It is monotonically growing. Once a claim is established, it is available forever. Multiple concurrent processes can tell and ask simultaneously; the evidence base coordinates them through entailment checking.

The difference from a bare constraint store is that Evident's evidence base stores *structured derivations*, not just constraint atoms. The evidence term for a claim records which sub-claims were used and how. This makes the evidence base richer than a standard CCP store — it preserves the full epistemic history, not just the current state.

The propagation loop — applying decomposition rules until fixpoint — is constraint propagation. When `card_valid` is established (told to the store), the propagator for `payment_authorized` checks whether all its other preconditions are now met. If yes, `payment_authorized` is established and told to the store. This cascade continues until quiescence. No search is involved; this is pure deduction.

Search only enters when there are competing decompositions (multiple `evident A because { ... }` clauses for the same claim) and propagation alone cannot determine which one applies. This is the residual nondeterminism after propagation has done all it can.

---

## CHR: Evident's Operational Core

Constraint Handling Rules (CHR, Frühwirth, 1991) are a rule-based formalism for writing constraint solvers. CHR rules fire when their head patterns match items currently in the constraint store. The rule transforms the store. The process continues until fixpoint.

Evident's decomposition rules are CHR propagation rules:

```
-- Evident
evident payment_authorized
    card_valid
    funds_sufficient
    merchant_not_blocked

-- Equivalent CHR propagation rule
card_valid, funds_sufficient, merchant_not_blocked ==> payment_authorized
```

When `card_valid`, `funds_sufficient`, and `merchant_not_blocked` are all present in the evidence base, the rule fires and adds `payment_authorized`. This is multi-head matching — the rule requires multiple facts to be simultaneously present, not just one. This is exactly what Evident's fan-in claims need.

CHR is not just an analogy for Evident — it should be Evident's operational semantics. The evidence base is a CHR constraint store. Claim decomposition rules are CHR propagation rules. Self-evident leaf claims are CHR built-in constraints dispatched to theory solvers. The fixpoint computation is the CHR execution loop.

What CHR adds that Evident needs to make explicit:
- **Simplification rules** (`⟺`): when an established claim is consumed to produce another (useful for linear/resource-sensitive evidence)
- **Multi-head matching**: claims depending on multiple simultaneously-established sub-claims
- **Guard conditions**: the `when` syntax already provides this — guards that must hold for a rule to fire
- **Composable theory plugins**: different constraint domains (arithmetic, strings, types) as swappable backends

---

## MiniZinc's Lesson: Decomposition is Yours, Search is the Runtime's

MiniZinc's programming model is the clearest statement of what Evident aspires to be. The programmer's job in MiniZinc:

1. Declare decision variables with their domains
2. Post constraints over those variables
3. Optionally provide search annotations as hints
4. Let the solver handle everything else

The compiler flattens the high-level model into a solver-neutral intermediate form (FlatZinc). The solver instantiates it against its available propagators and global constraints. The programmer never writes a search algorithm — they write a constraint model, and the system finds solutions.

MiniZinc's **global constraints** are the key to efficient solving. `alldifferent([x, y, z])` is not syntactic sugar for `x ≠ y ∧ x ≠ z ∧ y ≠ z`. It is a single constraint with a dedicated propagation algorithm that achieves full arc consistency using bipartite matching — reasoning about all variables simultaneously, not just pairs. The performance difference can be orders of magnitude.

For Evident, this suggests that **named claim patterns** should come with efficient built-in propagators. `all_different(vars)`, `no_overlap(intervals)`, `exactly_one(options)` — these should be first-class claim patterns with solver-backed implementations, not just syntactic abbreviations for their logical definitions.

**Redundant constraints** — logically implied facts that give the solver extra propagation hooks — are MiniZinc's mechanism for providing structural hints without changing the solution set. Evident needs an equivalent: a way to assert "you can also use this to propagate, even though it follows from what's already stated." The programmer marks these explicitly; the solver can use them or ignore them based on its internal heuristics.

---

## Angelic Nondeterminism: What Solver-Filled Claims Feel Like

Rosette's `solve` operation defines what it feels like to let the solver fill in a claim:

```racket
(solve (assert (valid-schedule? s)))
```

This says: find a binding for `s` such that `valid-schedule?` holds. The programmer writes what a valid schedule looks like. The solver finds one that is one. No scheduling algorithm is written. The solver "divines" values that make execution succeed — Rosette calls this **angelic nondeterminism**.

In Evident's syntax, this looks like:

```evident
? valid_schedule(jobs, ?assignment)
```

The `?assignment` is an output binding — find an `assignment` such that `valid_schedule(jobs, assignment)` is evident. The programmer has written the decomposition of `valid_schedule` (what conditions make a schedule valid). The solver finds a concrete assignment that satisfies all those conditions.

This works when the search space is bounded and the theory is decidable. For scheduling problems over a finite set of jobs and time slots with linear constraints on resource usage, this is tractable. For problems with unbounded search spaces or nonlinear arithmetic, the solver cannot guarantee an answer.

Sketch (Solar-Lezama, 2008) adds the CEGIS loop: **Counterexample-Guided Inductive Synthesis**. The synthesizer proposes a candidate; the verifier checks it against all possible inputs; if it fails, a counterexample is added to the synthesizer's constraints. The loop terminates when no counterexample exists. This is the mechanism by which "find me a program satisfying this spec" becomes tractable: restrict the search space (the programmer provides a sketch) and iterate.

For Evident, CEGIS suggests how to handle claims that the solver cannot immediately discharge: propose a candidate decomposition, verify it against known facts, add the contradiction as a new constraint, repeat. This is the solver's internal loop, invisible to the programmer.

---

## The Fundamental Asymmetry

Dafny reveals an important constraint on ambition. In Dafny, the programmer writes *both* the algorithm and the specification. Z3 verifies that they agree. The key finding: **verification is much easier than synthesis**. Checking that an algorithm satisfies a spec is an SMT query; finding an algorithm that satisfies a spec is a search problem that may be undecidable.

Evident's position in this space: Evident is not asking the solver to find arbitrary algorithms. It is asking the solver to fill in *leaf claims* — specific, bounded sub-problems in theories the solver handles. The decomposition hierarchy (the programmer's contribution) provides the algorithm's structure; the solver fills in the leaves.

This is closer to Sketch's hole-filling than to Dafny's verification. The programmer writes the shape; the solver fills in bounded constants, discrete choices, and arithmetic resolutions. The solver does not invent control flow, recursive structure, or arbitrary programs.

The implication for Evident's design: **the programmer must always decompose to the solver's vocabulary**. You cannot write `evident arbitrary_hard_problem(x)` and expect the solver to handle it. You must decompose until each sub-claim is in a theory the solver knows. The decomposition hierarchy is the algorithm; the solver handles what's left.

---

## What the Runtime Looks Like

Putting it together, Evident's runtime has three layers:

**Layer 1 — Evidence base (CHR constraint store)**
Monotonically growing set of established claims with their evidence terms. Claim decomposition rules are CHR propagation rules. The fixpoint loop runs until no new claims can be established. This is the application-level claim hierarchy.

**Layer 2 — Constraint propagation network**
For each established claim, propagators check whether downstream claims' preconditions are now met. When all preconditions for a claim are established, the claim is added to the evidence base (told to the store). This is the same fixpoint computation as Layer 1, but at the constraint level rather than the claim level.

**Layer 3 — Theory solvers (SMT backend)**
Leaf claims are dispatched to theory solvers: linear arithmetic (Z3's LIA solver), equality (congruence closure), algebraic data types (inductive term solver), and finite-domain enumeration (CP solver). These run deterministically when the claim is sufficiently bound. They may require search when it is not.

The programmer's code lives at Layer 1. The runtime manages Layers 2 and 3 automatically. The programmer's job is to decompose until every claim bottom out in Layer 3's vocabulary.

---

## What "Unknown" Means

Every solver architecture based on SMT must answer one question: what does it mean when the solver returns `unknown`?

Options:
1. **Runtime error**: the claim is neither established nor refuted; the program is ill-formed
2. **Suspension**: the claim is not yet determined; wait for more facts to be asserted
3. **Programmer error**: you must decompose further; `unknown` is a signal to add more structure

Option 3 is the right answer for Evident. An `unknown` response from the solver is a feedback signal: the claim is not yet sufficiently bound. The programmer must provide more decomposition, stronger guards, tighter type annotations, or explicit search hints. `unknown` is not a runtime failure — it is a design-time signal.

This preserves the key property: Evident's evidence base is always consistent (no claim is established as both true and false). `unknown` is a third state between established and refuted — the honest answer when the solver doesn't have enough information. The programmer eliminates `unknown` responses by providing more structure.

---

## The New Framing of Programming

Under this model, **programming in Evident is the process of reducing solver uncertainty**. You start with high-level claims that the solver cannot immediately determine. You decompose them into sub-claims, adding structure at each step. You continue until every sub-claim is either self-evident (directly decidable by a theory solver) or deterministically derived from other established claims.

A perfectly written Evident program is one where the solver never searches — it only propagates. Every claim follows by propagation from asserted ground facts through the constraint network. No branching, no backtracking, no guessing.

A well-written Evident program is one where the solver searches only over bounded, tractable sub-problems, and finds answers quickly.

A poorly written Evident program is one that asks the solver to search an unbounded space — or worse, to determine undecidable claims.

The feedback loop: when the solver returns `unknown`, you decompose more. When it returns answers too slowly, you add stronger constraints or provide search hints. When it returns answers instantly, you have structured the problem correctly.

This is not fundamentally different from what good programmers do in conventional languages — they break problems down until each piece is something the computer can handle directly. The difference is that in Evident, the "computer can handle directly" threshold is defined by a real constraint solver rather than by the primitive operations of a specific machine architecture. The primitive operations are logical, not mechanical.
