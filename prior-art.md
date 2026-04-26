# Prior Art

Evident synthesizes ideas from four distinct research traditions. None of them alone gets there; each contributes something essential.

---

## 1. Logic Programming

### Prolog (1972)

Prolog is the ancestral logic programming language. It represents programs as Horn clause rules — `head :- body₁, body₂, ...` — and executes them via SLD resolution: backward chaining from a goal, trying each matching rule in the order it appears.

**What it gets right:** The idea that a program can be read as a set of logical propositions, not an execution trace. The unification-based variable binding. The ability to run predicates "backwards" in some cases.

**What it gets wrong:** Clause ordering is semantically significant. The left-to-right, top-to-bottom strategy is baked into the language, not the runtime. The `cut` operator allows programmers to suppress backtracking, which destroys the declarative reading. In practice, writing correct Prolog requires holding the execution model in your head at all times.

The famous critique from Kowalski's 1979 paper "Algorithm = Logic + Control" acknowledged this directly: Prolog conflates what to compute (logic) with how to search (control), and the conflation is harmful. Evident treats this as a design axiom: the programmer specifies only logic; the runtime owns control.

### Mercury (1996)

Mercury adds a static mode system and determinism categories (`det`, `semidet`, `nondet`, `multi`) to a Prolog-like language. The compiler analyzes which arguments are inputs versus outputs and reorders clause bodies to find a valid execution strategy. If no valid ordering exists, compilation fails — the programmer is not permitted to write unordered code that happens to work.

This is the most serious prior attack on Prolog's ordering problem. It fails in a specific way: the solution is still a total order, just one the compiler chooses rather than the programmer. Evident's goal is more radical — partial order is the default, total order is a declared constraint.

### Datalog (1980s–present)

Datalog strips Prolog to its declarative core by forbidding function symbols (no compound terms) and side effects. This guarantees termination over finite domains. Crucially, Datalog uses bottom-up evaluation (compute the fixpoint of all rules over the fact base) rather than top-down SLD resolution. This makes rule ordering irrelevant: the same minimal model is always produced, regardless of the order rules are processed.

Datalog is fully order-independent within its domain. Its weakness is expressiveness: no compound data structures, no open-ended computations. It is the existence proof that the ordering problem is solvable in restricted settings.

Practical Datalog implementations — Soufflé (used for program analysis in Doop), Datomic's query language, Google's LogiQL — show that the model is industrially useful.

### Answer Set Programming (1988–present)

ASP (Gelfond and Lifschitz stable model semantics, implemented in Clingo and DLV) takes the most radical departure from Prolog. There is no search procedure visible to the programmer. You write rules, integrity constraints, and choice rules; the solver computes all *stable models* — consistent sets of beliefs — and returns those satisfying the constraints.

ASP supports both default negation (`not p` = "p cannot be proven") and classical negation (`-p` = "p is explicitly false"). Rule ordering is completely irrelevant. The computational engine is based on SAT-solving with constraint propagation (DPLL/CDCL), not depth-first search.

ASP is the clearest existence proof that full order-independence is achievable in logic programming. Its limitations: grounding (instantiating variables to concrete values) can be expensive, it does not naturally handle open-ended domains, and it is designed for combinatorial search problems rather than general-purpose computation.

### Constraint Logic Programming (CLP)

CLP extends Prolog by replacing unification with constraint solving. In `CLP(FD)` (finite domains), arithmetic constraints like `X + Y #= Z` can be posted in any order — the constraint solver figures out the satisfying assignment. This makes the constraint portion of a program order-independent. The surrounding Prolog control structure still imposes ordering; only the arithmetic is liberated.

CLP demonstrates a key point: order independence is achievable incrementally, domain by domain. A general-purpose language could provide this across all domains.

### miniKanren (2005)

miniKanren uses *interleaving search* rather than Prolog's depth-first search, making it complete: it will eventually find all answers, not just the ones a depth-first traversal would reach. It is deliberately minimal and embeddable. Its `cKanren` extension supports constraint posting in arbitrary order. miniKanren is a research platform showing that fair search and constraint-driven ordering are compatible.

---

## 2. Dataflow and Reactive Programming

### Static Dataflow Languages (1970s–1990s)

Lucid (1977), Val (1979), Id (1978), and SISAL (1983) are the foundational dataflow languages. Their core insight: if you eliminate mutable state and sequential ordering, what remains is a graph of value dependencies. A node fires when all its inputs are available. The execution order is derived from the dependency graph, not specified by the programmer.

These languages were mostly abandoned due to the difficulty of handling feedback (cycles), I/O, and general recursion. But their core idea — that the dependency graph *is* the program, and scheduling is an implementation detail — is exactly Evident's compute model.

LabVIEW (National Instruments, 1986) brought dataflow programming to instrument control engineers as a visual language. Wires between nodes *are* the dependency declarations. Any node executes as soon as all its inputs arrive. LabVIEW's persistence shows that non-programmer audiences find the dependency model more natural than sequential code.

### Functional Reactive Programming (1997–present)

FRP (Elliott and Hudak, ICFP 1997) models programs as transformations over time-varying values (behaviors) and discrete events. Composing behaviors implicitly constructs a dependency graph; the runtime evaluates it in topological order. Yampa (Haskell), Elm (originally), and Reactive Extensions (RxJS, RxJava) are practical descendants.

The key lesson from FRP's history: implicit dependency graphs are powerful but hard to reason about when dynamic (new dependencies added at runtime) or when historical values must be retained. Elm eventually abandoned FRP in favor of a simpler message-passing model. The tradeoff between expressiveness and learnability is real.

### Build Systems

Make (1976), Bazel (2015), and Shake (2012) are all dependency-first programs: you declare what each artifact depends on; the system computes the topological sort and executes only changed nodes.

Neil Mitchell's "Build Systems à la Carte" (ICFP 2018) formalizes the space of possible build systems by the algebraic structure of their dependency declarations. The key distinction: *applicative* build systems (Make, Bazel) have static dependency graphs fixed before execution. *Monadic* build systems (Shake) allow a rule's dependencies to depend on computed values — "what I depend on is itself a computed value." Monadic dependencies are more expressive but harder to analyze statically. This tradeoff — the `Applicative`/`Monad` distinction — appears in Evident too: simple evident claims have static dependencies; more complex ones may need to discover dependencies dynamically.

### Incremental Computation

Adapton (PLDI 2014) and Salsa (used in the Rust compiler) formalize incremental computation as a demanded computation graph: nodes are computations, edges are dependencies. When an input changes, only the affected downstream nodes are re-evaluated. Salsa powers incremental compilation in `rustc`, where a file change triggers re-evaluation of only the compiler queries that transitively depend on that file.

Spreadsheets (Excel) are the world's most widely-used dataflow system. Each cell formula declares dependencies on other cells; Excel maintains the dependency graph and propagates changes in topological order. The programmer specifies only "what depends on what"; Excel handles evaluation order, cycle detection, and incremental updates.

---

## 3. Type Theory and Proof Assistants

### The Curry-Howard Correspondence

The Curry-Howard correspondence (Curry 1950s, Howard 1969) establishes that propositions and types are the same thing, and proofs and programs are the same thing. A term `p : P` means that `p` is evidence — a concrete witness — that proposition `P` holds. Implication `A → B` is the function type: a function that, given evidence for `A`, produces evidence for `B`. Function application is modus ponens.

This is the theoretical backbone of Evident. "Evidence" in Evident is not a metaphor — it is precisely the constructive proof term that witnesses the truth of a claim.

### Proof Assistants and Tactics

Coq, Agda, Lean, and Idris are proof assistants built on dependent type theory. They expose two modes of proof construction:

- **Term mode**: write the proof term directly (write a program)
- **Tactic mode**: state the goal and issue commands that progressively simplify it

The tactic model is structurally identical to Evident's decomposition model. You state a high-level goal. A tactic like `split` decomposes conjunction `A ∧ B` into two subgoals. `apply f` reduces the goal from `B` to `A` given `f : A → B`. You recurse until each subgoal is closed by a trivially-true step (`rfl`, `exact`, `decide`). The original claim is established once all subgoals are closed.

This is exactly the "evident" operation: assert a name, decompose it into sub-claims, recurse until self-evident. The difference is that Evident is designed as a general-purpose programming language, not a proof assistant for mathematics.

### Holes and Metavariables

Proof assistants handle "fill this in later" via *holes* (Agda) and `sorry`/`admit` (Lean/Coq). A hole is a position in a proof with a known required type but no yet-provided term. The system typechecks the surrounding proof even with holes, reporting what remains to be filled. Evident takes this concept further: in a system designed for programming rather than formal verification, partial programs with holes are the normal state, not an error.

---

## 4. Rule-Based Systems

### Expert Systems and the Rete Algorithm

CLIPS (NASA, 1985) and Drools implement forward-chaining rule systems: IF conditions hold in working memory THEN assert new facts or trigger actions. Rules fire whenever their conditions are satisfied; the order of rule firing is not specified by the programmer (though conflict-resolution strategies impose a runtime order).

The Rete algorithm (Charles Forgy, 1974–1982) solves the performance problem. Rather than re-testing every rule against every fact on each cycle, Rete builds a dataflow network that incrementally maintains partial matches. When a fact changes, only the rules that could be affected are re-evaluated. The network structure encodes exactly the dependency graph between facts and rules.

This is Evident's forward-chaining execution model at scale. Rete's dataflow network is an automatically-constructed dependency graph over the rule set; Evident's dependency graph is explicitly derived from the claim decompositions.

### Constraint Handling Rules (CHR)

CHR (Frühwirth, 1991) extends Prolog with multi-headed rules that can simplify and propagate constraints. Rules fire when their heads match items in the constraint store, transforming the store. CHR is fully declarative within its constraint domain and is used for implementing custom solvers for scheduling, planning, and type inference.

CHR demonstrates that rule-based systems can be extended with multi-head matching (rules with multiple antecedents that match simultaneously) while remaining declarative. Evident's implication model needs a similar mechanism for claims that are evident only when multiple preconditions hold together.

### Term Rewriting Systems

Maude (and its predecessor OBJ) implements *rewriting logic*, where programs are sets of rewrite rules `l → r`. Computation proceeds by applying rules until no rule applies. Maude distinguishes equations (used for term simplification, must be confluent and terminating) from rewrite rules (may be nondeterministic, represent transitions).

The mathematical theory of term rewriting provides two key concepts for Evident:
- **Confluence**: different rule application orders reach the same result — order independence is guaranteed when the system is confluent
- **Termination**: the system always halts — provable via well-founded term orderings

These are exactly the properties Evident wants from its evaluation model. The TRS literature provides the technical machinery for proving when a rule system has these properties.

---

## Gap Analysis

No existing system combines all of Evident's goals:

| Property | Prolog | Datalog | ASP | FRP | Build Systems | Proof Assistants |
|---|---|---|---|---|---|---|
| Order-independent evaluation | No | Yes | Yes | Yes | Yes | Partial |
| First-class evidence/witnesses | No | No | Research | No | No | Yes |
| General-purpose computation | Yes | No | No | Yes | No | Yes |
| Top-down goal decomposition | Yes | No | No | No | Yes | Yes |
| Evidence = structured data | No | No | No | No | No | Yes |
| Designed for programmers, not logicians | Yes | Partial | No | Partial | Yes | No |

The intersection Evident targets — order-independent, evidence-first, general-purpose, decomposition-driven, programmer-accessible — is currently unoccupied.
