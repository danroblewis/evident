# Theoretical Foundations

Evident is grounded in three formal traditions: constructive logic (specifically the Curry-Howard correspondence), the semantics of dataflow computation, and the theory of term rewriting systems. This document traces the connections.

---

## 1. The Evident Judgment

The central notion in Evident is a **judgment** of the form:

```
Γ ⊢ A evident
```

Read: "given context Γ, claim A is evident."

This is analogous to the typing judgment `Γ ⊢ t : T` in type theory, which says "term `t` has type `T` in context `Γ`." Here, `A` plays the role of the type/proposition, and the evidence term (derivation tree) plays the role of the term/proof. In fact, under the Curry-Howard correspondence, these judgments are the same thing viewed differently.

The context Γ is the set of facts already established — the "working memory" of the system. Adding a new fact to Γ is monotonic: it can only make more things evident, never fewer. (Non-monotonic extensions, where new facts can retract old conclusions, are an open design question discussed in [Open Problems](open-problems.md).)

---

## 2. Evidence as Witness (Curry-Howard)

In constructive (intuitionistic) logic, a proof of a proposition is not just a certificate that the proposition is true — it is a *witness*, carrying the computational content of *why* it is true.

The isomorphism (Curry-Howard correspondence):

| Logic | Computation |
|---|---|
| Proposition `A` | Type `A` |
| Proof of `A` | Value of type `A` |
| `A → B` (implication) | `A → B` (function type) |
| `A ∧ B` (conjunction) | `A × B` (product/pair type) |
| `A ∨ B` (disjunction) | `A + B` (sum/variant type) |
| `⊥` (absurdity) | Empty type / `Void` |
| `∀x. P(x)` | Dependent function type `(x : A) → P(x)` |
| `∃x. P(x)` | Dependent pair type `Σ(x : A). P(x)` |

In Evident's terms: when a claim `A` is established, the evidence term for `A` is a first-class value. If `A` was established by decomposing it into `B` and `C`, the evidence for `A` is a pair `(ev_B, ev_C)`. If `A` was established from `B` via a rule `B → A`, the evidence is a function application `rule(ev_B)`.

This has practical consequences:
- You can inspect the evidence for any established claim: not just *that* it holds, but *why*
- Evidence can be logged, tested, stored, and compared
- Two different derivation paths for the same claim produce structurally different evidence terms — which may or may not matter (see proof relevance in [Open Problems](open-problems.md))

### Existential Claims and Witnesses

When `A` is an existential claim — "there exists some X such that P(X)" — the evidence for `A` must include a concrete witness `x` together with evidence that `P(x)` holds. This is the Sigma type `Σ(x : A). P(x)`. You cannot prove existence without producing the witness; indirect arguments by contradiction are not valid in constructive logic.

This maps cleanly to Evident: establishing `payment_processor_available` requires producing an actual processor identifier and evidence that it is currently accepting transactions.

---

## 3. The Dependency Graph as Semantics

Evident's operational semantics can be described as a **demanded computation graph** (following Adapton's terminology):

- **Nodes** are claim names (or parameterized claims — predicates applied to values)
- **Edges** are dependencies: A depends on B means B must be evident before A can be established
- **Leaf nodes** are base facts — either axiomatically evident or established by external input

A program in Evident implicitly defines this graph. The decomposition rules:

```
evident A because { B, C, D }
```

adds directed edges `A → B`, `A → C`, `A → D` to the graph. Any valid topological ordering of the graph is a valid evaluation strategy.

### Monotonicity and Fixpoints

Because establishing facts is monotonic — the set of evident claims can only grow — Evident's semantics can be described as a fixpoint computation. Starting from the initial fact base, repeatedly apply all applicable decomposition rules until no new claims can be established. The result is the **minimal model**: the smallest set of claims that is closed under the rules.

This is exactly the semantics of Datalog, and it is what makes evaluation order irrelevant: the same minimal model is produced regardless of which rules are applied in which order, as long as all applicable rules are eventually applied.

Formally, if `T` is the consequence operator (given a set of established facts, `T(S)` is the set of facts derivable in one step), the semantics of an Evident program is:

```
⟦P⟧ = lfp(T) = T(∅) ∪ T(T(∅)) ∪ ... = ⋃ᵢ Tⁱ(∅)
```

The least fixpoint exists because `T` is monotone over the lattice of sets of facts ordered by inclusion.

---

## 4. Confluence and Order Independence

The key theorem that makes order independence possible:

**Theorem (Church-Rosser / Confluence):** If a term rewriting system is confluent, then any two reduction sequences starting from the same term produce the same normal form (if they terminate).

In Evident's terms: if the decomposition rules are confluent — no two rules produce contradictory evidence — then the order in which rules are applied does not change the set of ultimately established claims.

Confluence is guaranteed in the monotonic setting (facts are only added, never retracted). Non-monotonic rules (where new facts could retract old ones, as in defeasible logic) can break confluence and require explicit conflict resolution. This is a core design tension.

### The Diamond Property

The intuitive picture of confluence is the diamond: if starting from state S you can reach both S₁ and S₂ by different single steps, there must exist some state S' reachable from both S₁ and S₂. In graph terms: if the dependency graph is a DAG (no cycles), any topological ordering terminates at the same leaf set.

---

## 5. Decomposition as Directed Proof Search

Evident's computational model is equivalent to **backward-chaining proof search** in a specific logic, with two key modifications:

1. **No committed choice**: in standard Prolog, once a clause is selected, alternatives are deferred (explored only on backtracking). In Evident, multiple decompositions for the same claim may be explored simultaneously or in any order, and the first to succeed establishes the claim.

2. **Evidence is retained**: the derivation tree is preserved as a first-class value, not discarded after the goal is proved.

### The Self-Evidence Relation

Some claims are **self-evident**: they require no decomposition because they are directly checkable. Self-evident claims are the leaves of the dependency graph. Examples:
- Arithmetic equalities that normalize to a common form (`2 + 2` evidences as `4` by definitional reduction)
- Comparisons between concrete values
- External facts asserted as axioms

The distinction between "self-evident" and "requires decomposition" maps to the type-theoretic distinction between *definitional equality* (established by reduction, no proof required) and *propositional equality* (requires an explicit proof term). In Lean, `rfl` closes goals that are definitionally equal — they are self-evident. Goals requiring `omega` or custom lemmas require explicit evidencing.

---

## 6. Implication Chains and Program Architecture

The primary programming act in Evident is writing implications: `A → B`, read "if A is evident, then B is evident." Implications compose via transitivity: `A → B` and `B → C` yields `A → C` (hypothetical syllogism).

Complex programs emerge as chains and trees of implications:

- **Chains** `A → B → C → D`: a pipeline where each stage adds evidence
- **Fan-out** `A → B` and `A → C`: A's evidence supports multiple conclusions
- **Fan-in** `(B ∧ C) → D`: conjunction of evidences produces a new evidence

The claim-decomposition syntax:

```
evident D because { B, C }
```

is exactly the reverse implication: `(B ∧ C) → D`, written from the conclusion. This top-down reading ("to establish D, establish B and C") is dual to the bottom-up forward-chaining reading ("when B and C are both established, D follows").

### Implication Trees and Linked Lists

A pure implication chain `A → B → C → D` is isomorphic to a linked list: each node has exactly one successor, and the chain terminates at an axiom. A branching implication structure — where each node has multiple preconditions — is isomorphic to a tree. The claim hierarchy the user envisioned maps exactly to this: parent claims are supported by the conjunction of their child claims.

This structure has a natural type-theoretic reading: the claim name is a type, the evidence for the claim is a value of that type, and the decomposition into sub-claims is a record/product type where each field holds the evidence for a sub-claim.

---

## 7. The Relationship to Horn Clauses

Prolog's Horn clause restriction — rules have exactly one positive literal (one conclusion) — is what makes Prolog's backward chaining well-defined. If rules could have multiple conclusions (`A → B ∨ C`), the search procedure would need to choose which conclusion to establish, introducing nondeterminism at a fundamentally harder level.

Evident embraces this restriction for the same reason: each decomposition rule has exactly one named conclusion. What Evident liberates is not multiple-conclusion rules, but the *ordering* of how rules are tried and the *retention* of evidence.

The connection to Horn clauses means Evident programs can be interpreted as logic programs, and results from the logic programming literature (termination proofs, model theory, complexity bounds) apply directly.
