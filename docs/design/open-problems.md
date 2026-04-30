# Open Problems

These are the hard questions that would need to be resolved to turn Evident from a concept into a working language. They are genuine design challenges, not minor implementation details.

---

## 1. Termination

**The problem**: Implication chains can cycle. If `A → B → A`, the system loops. This is the logic programming analog of infinite recursion. In Prolog, left-recursive rules like `ancestor(X, Z) :- ancestor(X, Y), parent(Y, Z)` loop immediately. In a fixpoint-based system, detecting the cycle prevents infinite looping, but it may also prevent finding valid derivations.

**Partial solutions from the literature:**
- **Tabling/memoization** (XSB Prolog, Souffle): cache intermediate results and don't re-enter a subgoal already being computed. This handles mutually recursive rules but doesn't guarantee termination in all cases.
- **Stratification** (Datalog): partition rules into layers where each layer only depends on claims from lower layers. Cycles are only allowed within a layer. This guarantees termination but restricts expressiveness.
- **Well-founded orderings**: require a programmer-declared measure that strictly decreases with each recursive application. This is the standard approach in proof assistants (structural recursion in Coq, termination metrics in Agda).
- **Fuel/step limits**: bound the maximum depth of derivation and treat deeper derivations as "not evident." Pragmatic but theoretically unsatisfying.

**The design question**: Should Evident require the programmer to provide termination proofs (as proof assistants do), or should it detect cycles at runtime (accepting that some programs may fail to terminate), or should it restrict the language to guarantee termination (as Datalog does)?

---

## 2. Non-Monotonicity and Retraction

**The problem**: The monotonic semantics (facts only grow, never shrink) is clean but limiting. Real programs need to handle changes: a fact that was true may become false. A payment authorized yesterday may be declined today. A cached result may expire.

Monotonic Evident cannot express "this fact is no longer evident." To handle updates, the programmer would need to timestamp or version facts and include freshness conditions in every derivation that cares about time.

**Approaches from the literature:**
- **Defeasible logic**: rules have priorities; higher-priority rules override lower-priority ones without retracting them. Works for static conflict resolution but doesn't handle dynamic retraction.
- **Truth maintenance systems (TMS)**: the runtime tracks which facts depend on which assumptions. When an assumption is retracted, all facts that depended on it are also retracted. This requires maintaining a justification graph over the entire fact base — expensive but principled.
- **Event sourcing / append-only**: represent changes as new facts ("as of time T, card was declined") rather than retracting old ones. Pure monotonicity, but queries become more complex.

**The design question**: Is Evident purely monotonic (simpler, closer to Datalog), or does it support retraction (more expressive, much more complex)? Non-monotonic Evident loses the clean fixpoint semantics and confluence guarantees; it would need a conflict-resolution mechanism analogous to ASP's priority ordering or defeasible logic's specificity principle.

---

## 3. I/O and Side Effects

**The problem**: Real programs communicate with the outside world. Fetching a URL, reading a file, writing to a database — these are side effects. In a purely declarative, order-independent system, side effects create a problem: if the order of rule evaluation is unspecified, and two rules have side effects that interact, the behavior becomes unpredictable.

This is the same problem functional programming faces, solved differently in different traditions:
- **Haskell**: `IO` monad — side effects are explicit, sequenced by monadic bind
- **Rust**: `async/.await` — side effects are deferred and their dependencies are explicit
- **Linear logic**: consuming a fact means using a resource exactly once; this models I/O as a linear resource

**For Evident**, several approaches are possible:
1. **Impure leaves**: self-evident claims can have side effects; the system constrains their order by dependency declarations. If `fetch_user` and `fetch_order` are independent, they may be fetched in parallel.
2. **Effect tokens**: treat I/O as a resource (linear logic style). The "world token" threads through I/O operations, forcing a total order among them.
3. **Explicit sequencing**: provide a sequencing operator `A then B` for when order genuinely matters, orthogonal to the dependency-based order.

The risk of (1) is nondeterministic behavior when side effects have hidden interactions. The cost of (2) and (3) is that the programmer must explicitly sequence I/O, partially giving up the order-independence promise.

---

## 4. Proof Relevance

**The problem**: Can the same claim be established in multiple ways, and if so, does it matter which way was used?

Consider `sorted([1, 2, 3])`. There is exactly one derivation: the one that checks `1 ≤ 2` and `2 ≤ 3`. But for `even(4)`, there might be multiple derivations: `4 = 2 + 2` and `4 = 0 + 4` and `4 = 6 - 2`. Are these the same evidence?

In **proof-irrelevant** systems (like Haskell's proposition squashing), all proofs of the same proposition are considered equal — only the truth of the proposition matters. In **proof-relevant** systems (Homotopy Type Theory), different proofs of the same proposition can be genuinely distinct mathematical objects.

For Evident the practical question is: can a consumer of evidence depend on *how* a claim was established, or only on *that* it was established? If evidence is proof-irrelevant, the runtime is free to choose any derivation path. If evidence is proof-relevant, the consumer may care about the specific derivation, which constrains the runtime's freedom.

One design: allow the programmer to declare claims as proof-relevant or proof-irrelevant. Proof-irrelevant claims can be established by any derivation; proof-relevant claims carry the specific derivation in their evidence term. The default would be proof-irrelevant (matches the casual intuition that "it doesn't matter how you know, only that you know").

---

## 5. Dynamic Dependencies

**The problem**: In many programs, what a claim depends on is itself a computed value. A query that needs to fetch a configuration file before it knows what database to connect to cannot declare its dependencies statically.

This is exactly the applicative/monadic distinction from Neil Mitchell's "Build Systems à la Carte": a monadic dependency is one where "the set of things I depend on is itself a computed value." Monadic dependencies are more expressive but harder to analyze statically.

In Evident, pure decomposition rules have static dependencies — you write `evident A because { B, C }` and the dependency on `B` and `C` is fixed. But what if whether A depends on B or C depends on the value of some other claim?

**Possible approach**: Allow a limited form of dynamic evidence — claims that, when established, assert new rules or new facts. This is the "clause assertion" mechanism in Prolog (`assert/1`, `retract/1`), which is its most powerful and most dangerous feature. In Evident, dynamic rule assertion would need to be carefully bounded (perhaps restricted to monotonic additions) to preserve confluence.

---

## 6. Composability and Modularity

**The problem**: Large programs need to be organized into modules. In a rule-based system, rules from different modules can interact in unexpected ways — a rule in module B might fire on facts produced by module A, creating a hidden dependency between modules.

**Design tension**: The power of Evident comes from rules being globally visible (any rule can fire on any matching fact). Modularity requires restricting this — some rules should only fire within their module.

Approaches:
- **Namespaced claims**: claims are namespaced to their module; cross-module interaction requires explicit imports
- **Scoped evidence**: a claim's evidence is only visible within its declaring scope
- **Open/closed rules**: rules are explicitly marked as open (matchable from outside) or closed (only matchable within the module)

The goal is to preserve the order-independence and compositional semantics within modules while providing encapsulation between them.

---

## 7. Negation and the Open-World Assumption

**The problem**: Evident's default closed-world assumption (anything not provable is false) is convenient but wrong for many domains. In an open-world setting, failure to prove X does not mean X is false — it means you don't have enough information.

Consider building a permissions system: `user_has_permission(user, action)` should fail if there is no evidence of the permission, not if the permission is explicitly absent. The closed-world reading ("no evidence = denied") is the right default for security. But for a knowledge graph query, the open-world reading ("no evidence = unknown") may be more appropriate.

ASP's distinction between `not p` (negation as failure, closed-world) and `-p` (classical negation, explicit falsification) addresses this at the language level. Evident would benefit from a similar distinction.

---

## 8. What Does "Evident" Add to What We Have?

The sharpest version of the opposing view: is Evident just Datalog with better syntax and first-class evidence terms? 

The honest answer is: those are the two most important design innovations, but they are not trivial. First-class evidence (proof terms as data) is absent from all practical logic programming systems. The combination of:
- Ordered-independence (from Datalog/ASP)  
- First-class evidence (from type theory)
- General-purpose computation (unlike Datalog's finite-domain restriction)
- Top-down decomposition as the primary design act (unlike Datalog's bottom-up focus)

...has not been packaged together before. Whether packaging them together creates something qualitatively new, or just a more convenient combination of existing ideas, is an empirical question that only a working implementation can answer.

## Debugging and observability

When a schema is unsatisfiable or returns unexpected results, there is no good way to know why. Specific gaps:

- **Field discoverability**: sub-schema fields like `task.duration` are in the flat bindings but invisible until samples arrive. No UI tells you they exist.
- **Sub-schema contribution**: which child constraints were binding vs. had slack? The `Evidence` tree in `runtime/src/evidence.py` records this but is never shown.
- **Why unsat?**: Z3 can produce an unsat core (minimal conflicting constraints) — not yet exposed.
- **Schema shape before sampling**: axis dropdowns populate from the first sample's keys; you can't explore the space before sampling runs.

The multi-`@plot` projection pattern (see `ide/examples/scheduling-views.ev`) is a step toward this. The `view` syntax (noted elsewhere) would formalize it. The `Evidence` tree is the right runtime foundation to build on.
