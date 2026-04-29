# SMT-Backed Languages: Research Notes for Evident

## The Landscape

Six systems define the state of the art for programming languages that use SMT solvers as a computational backend. Each represents a different answer to the same question: how much of the program can the solver fill in, and what does the programmer still have to write?

---

## 1. Sketch: Holes as the Primitive

Sketch (Solar-Lezama, 2008) is the origin point. The programmer writes a *sketch* — a program with holes marked `??` — alongside a specification, typically a reference implementation or a set of assertions. The synthesizer completes the sketch by finding concrete values for the holes that make the program satisfy the specification across all inputs.

The hole is intentionally small. A `??` in Sketch denotes an unknown integer constant within a bounded range. By composing holes with array indexing — `i*?? + j*??` — the programmer implicitly defines a *search space* of candidate expressions without enumerating them. The synthesizer explores that space using CEGIS: counterexample-guided inductive synthesis. CEGIS alternates between two phases. The *synthesis oracle* finds a candidate program that satisfies all examples seen so far. The *verification oracle* checks the candidate against all inputs; if it finds a counterexample, that input is added to the example set and the loop repeats. The loop terminates when no counterexample exists — the program is correct — or when the search space is exhausted.

The key design insight: the programmer specifies the *structure* of the program (which operations appear, in what shape) and the *correctness criterion*, but not the specific constants. The solver fills in exactly those constants. This is a narrow but precise division of labor. Sketch does not ask the solver to invent control flow; the programmer provides the skeleton, and the solver instantiates the parameters.

---

## 2. Rosette: Solver-Aided Programming as a Library

Rosette (Torlak and Bodik, 2013–2014) generalizes Sketch into a full language framework embedded in Racket. Rosette introduces *symbolic values* — placeholders with unknown concrete values — and allows normal Racket programs to execute over them. When a program runs on symbolic inputs, it produces a *constraint*, not a result. Rosette then dispatches one of four queries to an SMT solver:

- **`verify`**: find a binding for symbolic constants that causes an assertion to fail (counterexample search).
- **`synthesize`**: find a binding that makes a sketched program satisfy a specification.
- **`solve`**: find a binding that causes the program to execute without any assertion failures (satisfying execution search).
- **`debug`**: given a failing execution, localize the root cause.

The `solve` query implements *angelic nondeterminism*. The programmer writes a program with symbolic unknowns and asserts desired properties; the solver "divines" values that make execution succeed. The experience is that the program runs as if an oracle made all the right choices. This is genuinely different from ordinary execution: you write `(solve (assert (valid-schedule? s)))` and get back a concrete schedule. You did not write a scheduling algorithm; you stated what a valid schedule looks like, and the solver found one.

Angelic nondeterminism is powerful because it inverts computation: you specify the output property, and the solver works backwards to find an input. This is only tractable when the search space is bounded and the constraint theory is decidable. Rosette quietly encodes everything to bit-vector logic, making it decidable-but-expensive rather than potentially undecidable.

---

## 3. Dafny: The Dual-Writing Problem

Dafny (Leino, 2010) occupies a different point in the design space. The programmer writes *both* the algorithm and the specification. Z3 checks that they match. This is not synthesis — the solver does not find the algorithm. It checks that the algorithm the programmer wrote satisfies the specification the programmer also wrote.

Dafny specifications use three constructs:
- `requires` — preconditions: what must be true before a method is called
- `ensures` — postconditions: what the method guarantees on return
- `invariant` — loop invariants: properties preserved across every loop iteration

Z3 handles the checking automatically for most arithmetic and logical properties. The friction arises from *loop invariants*. Z3 cannot infer loop invariants on its own; the programmer must supply them. This is not a limitation of Z3 but a fundamental one: loop invariant inference is undecidable in general. The programmer must therefore think through the invariant structure of every loop — a significant cognitive burden, and the primary source of friction in Dafny programs.

A second friction point is the *annotation cascade*. When a method calls another method, Dafny only knows what the callee's postcondition promises — not what its implementation does. This forces the programmer to strengthen postconditions progressively to expose the facts downstream callers need. Every abstraction boundary requires explicit specification of what crosses it.

The comparison to Evident is sharp: Dafny requires writing both the `how` (algorithm) and the `what` (specification), then proving they agree. Evident aspires to require only the `what`, with the runtime finding a `how`. That is a much harder problem, and Dafny's architecture reveals why: the solver can *check* that an algorithm satisfies a spec far more easily than it can *find* an algorithm that does.

---

## 4. Liquid Haskell: Types That Flow

Liquid Haskell (Rondon, Kawaguchi, and Jhala, 2008; extended for Haskell by Vazou et al.) attaches logical predicates directly to Haskell types. A *refinement type* is an ordinary type decorated with a predicate:

```haskell
{-@ type Pos = {v:Int | v > 0} @-}
{-@ divide :: Int -> Pos -> Int @-}
```

The name "liquid" comes from "Logically Qualified Data Types" — predicates flow through the type system like a liquid, propagating invariants from definition sites to use sites. When you call a function returning `{v:Int | v >= 0}` and pass the result to a function expecting `{v:Int | v > 0}`, Liquid Haskell generates the subtyping obligation `v >= 0 => v > 0` — unprovable without more context, which forces the programmer to strengthen the type somewhere upstream.

Liquid Haskell sends *subtyping obligations* to Z3 — quantifier-free formulas in linear integer arithmetic with uninterpreted functions (QF-UFLIA). This fragment is decidable and efficiently decidable, which is the core design decision. By restricting refinement predicates to this fragment, Liquid Haskell guarantees that type-checking terminates and that Z3 will always produce an answer. What falls outside QF-UFLIA — nonlinear arithmetic, universal quantification over recursive data structures, higher-order properties — requires manual hints: lemmas written as Haskell functions whose types encode the needed fact.

The programmer experience is: most of the time, you annotate a type and Z3 discharges the obligation silently. Occasionally, you write a helper lemma. The lemma's *type* is the logical statement; its *body* is the proof, which Liquid Haskell checks. The programmer is never directly aware of Z3 unless something fails to verify.

---

## 5. F*: When the Solver Is Not Enough

F* (Swamy et al., Microsoft Research) uses Z3 as its primary proof automation engine, but unlike Liquid Haskell, F* does not restrict itself to a decidable fragment. F* sends first-order logic with quantifiers and uninterpreted functions to Z3 — a logic in which proof-finding is undecidable. This gives F* more expressive specifications at the cost of solver instability.

F*'s boundary between automatic and manual proof is empirical, not theoretical. Some properties Z3 handles automatically — arithmetic, basic logical deductions, well-founded termination arguments. Others require programmer intervention:

- *Nonlinear arithmetic* (multiplying variables together): F* documentation explicitly recommends disabling Z3's nonlinear reasoning and using manual lemmas from `FStar.Math.Lemmas` instead, because Z3's nonlinear heuristics are unpredictable.
- *Deeply recursive functions*: Z3 unrolls recursive definitions to a fixed "fuel" depth; if a proof requires more unrolling than the fuel allows, the programmer must provide explicit unfolding hints.
- *Quantifier instantiation*: Z3 uses pattern-based heuristics to instantiate quantifiers; when patterns cause matching loops or fail to fire, proofs time out.

F* provides an escape hatch in both directions. *Tactics* (Meta-F*) allow the programmer to manipulate proof goals programmatically — reducing the goal to something Z3 can handle, or filling in the proof term directly in the style of Coq. *Hint files* cache Z3's proof search so that stable proofs can be replayed cheaply without re-running the solver.

The key lesson from F*: allowing undecidable theories gives expressiveness but creates a fragile engineering situation. Proofs can pass on one version of Z3 and time out on the next. The programmer must learn to write "Z3-friendly" specifications — a deep expertise that leaks the solver's internal heuristics into the programming model.

---

## 6. The "Sufficiently Constrained" Condition

All of these systems face the same boundary. The solver can answer your query if and only if:

1. The query falls within a decidable theory (or a fragment Z3 has complete heuristics for), and
2. The problem size is tractable (the SAT/SMT instance is not exponentially large).

The decidable theories Z3 handles well — and that are most useful for general-purpose programming — are:
- **Linear integer and real arithmetic** (QF-LIA, QF-LRA): addition, subtraction, comparison. No multiplication of variables.
- **Bitvector arithmetic** (QF-BV): machine arithmetic, shifts, bitwise operations.
- **Arrays** (QF-AX): read/write operations with extensionality.
- **Uninterpreted functions** (QF-UF): equality reasoning without interpretations.
- **Combinations** thereof (QF-AUFLIA, etc.).

What pushes queries out of the tractable zone:
- **Nonlinear arithmetic**: `x * y = z` with all three variable triggers undecidability over integers.
- **Quantifiers over infinite domains**: `forall x. P(x)` with x ranging over integers requires instantiation heuristics that can loop.
- **Recursive functions without fuel bounds**: Z3 cannot reason about arbitrary recursion depth.
- **Heap-dependent properties**: aliasing and mutation are notoriously hard to encode in SMT.

The programmer's experience of hitting this boundary varies by system. In Dafny, Z3 times out and the verifier reports "could not be verified." In Liquid Haskell, the type-checker reports a subtyping failure because the constraint is outside the decidable fragment. In F*, the solver returns `unknown`. In Rosette, the symbolic execution terminates but the constraint becomes unsolvable.

---

## 7. The Oracle Model

The informal model used in the SMT-backed language literature describes the solver as an *oracle*: a black box that the programmer queries with a formula and that returns `SAT` (with a witness), `UNSAT` (with a proof), or `unknown`. The oracle model has three important properties:

- **The oracle has no memory**: each query is independent. The programmer cannot build up context across queries incrementally (though SMT solvers have incremental interfaces, programming languages rarely expose them cleanly).
- **The oracle can say "I don't know"**: when the query is outside the solver's decidable fragment or exceeds its time budget, it returns `unknown`. This is not a bug — it is the honest answer. The programmer must then either reformulate the query or add more constraints.
- **The oracle can be wrong-by-timeout**: a formula that is UNSAT but takes exponential time to prove looks, from the outside, like `unknown`. Experienced SMT programmers know to distinguish "it timed out at 10 seconds" from "it timed out at 10 minutes" — the latter suggests the formula is genuinely hard, not just slow to start.

The oracle model creates a characteristic friction pattern in all these languages: the programmer writes a high-level specification, the oracle discharges it, and then — occasionally and unpredictably — the oracle fails. The programmer must then descend into the oracle's internal world (constraint theories, quantifier patterns, unrolling depth) to reformulate the query. This is a significant abstraction violation: the programmer chose a high-level language to avoid reasoning about constraint solving, and now must do exactly that.

---

## Implications for Evident

**What works well in these systems:**

- The angelic nondeterminism model (Rosette's `solve`, Sketch's holes) gives programmers a genuine experience of "write what you want, get it." This works when the search space is bounded and the constraint theory is decidable. For Evident, this suggests that some subcategory of claims — those with finite, well-typed solution spaces — could be filled in by a solver without the programmer specifying how.

- Liquid Haskell's approach of restricting to a decidable fragment and generating implicit queries is ergonomically clean. The programmer thinks in terms of types and properties; Z3 is invisible unless something fails. Evident's "self-evident" leaf nodes are analogous: within a constrained domain (linear arithmetic, equality), the runtime could discharge them automatically.

- Dafny demonstrates that the specification-first model is viable for checking correctness but not (yet) for finding programs. Evident's claim decomposition is specification-first; its execution model (top-down goal-driven search) is closer to Prolog than to Dafny's Z3-based verification.

**Where friction is likely for Evident:**

- The boundary between "the runtime figures this out" and "the programmer must specify this" will need to be explicit and predictable. Every SMT-backed language suffers from the oracle's `unknown` response leaking through the abstraction. Evident will need to decide: is an unsolvable sub-claim a runtime error, a compile-time error, or a signal to request more specification from the programmer?

- Constraint theories for general-purpose programming are narrower than programmers expect. Integer arithmetic with multiplication, recursive data structures, and heap mutation are all outside the tractable SMT zone. Evident's execution model based on Horn clauses and fixpoint computation is actually better positioned than Z3-based checking for these cases — but combining the two (using Z3 for leaf-level constraint checking while using tabling/fixpoint for higher-level claims) is an open engineering problem.

- The "write only the spec" promise — as opposed to Dafny's "write spec and implementation" — is Evident's most ambitious claim. Sketch achieves it for bounded constant-filling; Rosette achieves it for bounded search; neither achieves it for general recursive programs. Evident's tabling + fixpoint execution model may provide a path, but the solver boundary remains.

**Most useful constraint theories for Evident's leaf nodes:** linear integer arithmetic (for numeric guards and comparisons), equality with uninterpreted functions (for structural matching), and finite-domain enumeration (for discrete choice). These three theories cover a large fraction of the constraints that appear in real programs and are all decidable and tractable.
