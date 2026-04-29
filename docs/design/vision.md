# Vision: Why Evident

## From Callbacks to Dependency Graphs

JavaScript programmers discovered something important, slowly and by accident.

In the beginning, there was `setTimeout(fn, 0)`. This wasn't a timer — nobody cared about zero milliseconds. It was a way to push an operation onto the end of the task queue, *after* whatever was currently running. It was ordering control dressed up as time control. If your callback depended on a DOM mutation having propagated, you called `setTimeout(fn, 0)` and hoped.

Then came Promises. Instead of threading a callback through every function signature, you could write `fetch(url).then(parse).then(render)`. This encoded a dependency chain: `render` depends on `parse` which depends on `fetch`. The runtime would respect that dependency without the programmer having to reason about when the event loop might get around to it.

`async/await` completed the transformation. Syntactically, it looks like sequential code. Semantically, it's a compiler-managed dependency graph. The `await` keyword doesn't mean "pause everything" — it means "this continuation cannot proceed until this value is available." The programmer specifies *dependencies*, and the runtime handles *scheduling*.

This is the insight at the heart of Evident: if you specify only the dependency structure, you give the runtime the freedom to do everything else. Scheduling, parallelism, optimization — these become the system's problem.

---

## What Prolog Got Right, and What It Got Wrong

Prolog was a genuine breakthrough. The idea that a program could have both a *declarative reading* (what is true) and a *procedural reading* (how to compute it) was profound. You wrote logical facts and rules, and the machine found answers. The programmer should be able to say `ancestor(X, Y)` without specifying the order in which relatives are searched.

In practice, this vision broke down almost immediately.

Prolog executes clauses top-to-bottom and goals within clause bodies left-to-right. This makes ordering semantically meaningful. The logically identical program:

```prolog
ancestor(X, Z) :- ancestor(X, Y), parent(Y, Z).  % loops
ancestor(X, Z) :- parent(X, Y), ancestor(Y, Z).  % works
```

behaves completely differently depending on which goal comes first. Programmers learned to write Prolog with the execution order in their heads at all times — using it as an esoteric procedural language with a declarative syntax painted on top.

The `cut` operator made it worse. Cut commits to a branch and eliminates backtracking — it is pure procedural control embedded in a logic language. Real Prolog programs are full of cuts, negation-as-failure, and explicit control structures. The declarative and procedural readings diverged completely.

Evident takes the opposite stance: ordering is never a property of the source program. If two facts can be established independently, the system may establish them in any order, or simultaneously. If the programmer needs a specific ordering, they must declare it explicitly — as a dependency.

---

## Go's Lesson: Disorder as a Feature

When the Go team made map iteration order nondeterministic — randomized on each run — they received complaints. Maps in other languages were reliably ordered. Programs that depended on map ordering often worked fine for months.

That was the point. Programs that accidentally depended on map ordering were already broken; they just hadn't noticed yet. The random ordering made the implicit assumption visible as an immediate failure, rather than a latent bug waiting for a slightly different runtime environment.

This is a design philosophy: if a program's correctness depends on an ordering the specification does not guarantee, that dependency is a bug, not a feature. Making the ordering random forces programmers to be explicit about the constraints they actually need.

Evident extends this philosophy to the language level. There is no default ordering between independent claims. The system will establish them in whatever order is most convenient. If your program breaks under a different evaluation order, it was never correct — it just had an undeclared dependency.

---

## The Evident Metaphor

Consider how humans recognize that something is *obvious* or *self-evident*. Some things need no argument — you look at them and they are immediately established. Others require a chain of reasoning, but once you trace the chain, the conclusion feels inevitable. "Of course — given those premises, this follows."

The word "evident" is used deliberately. In formal epistemology, evidence is what justifies belief. In constructive logic, a proof is not just a certificate that something is true — it is the *witness*, the *evidence itself*. To show that a sorted list exists, you must produce the list and the comparison chain that shows each adjacent pair is ordered.

In Evident, every established fact carries its evidence. If `http_request_succeeded` is evident, there is a concrete derivation showing *why* — which status code was checked, which body property was inspected. This is not just a debugging convenience. Evidence is a first-class value: you can inspect it, pass it, and reason about it.

The workflow is:
1. Name something you want to be true at a high level: `payment_authorized`.
2. Ask: is this directly evident from what we know? If yes, done.
3. If not, provide a decomposition: `payment_authorized` is evident when `card_valid` and `funds_sufficient` and `merchant_not_blacklisted` are all evident.
4. Each of those may require further decomposition.
5. Eventually every leaf is directly checkable, and the tree of sub-evidences constitutes the proof of the original claim.

This is top-down specification that naturally produces a dependency graph. The tree structure emerges from the decomposition, not from the programmer explicitly constructing a DAG.

---

## Implications Over Sequences

The primary computational primitive in Evident is not assignment, not function call, not statement sequence. It is *implication*: if A is evident, then B is evident.

Implication is directional. `A → B` does not mean `B → A`. But it is not sequential — `A → B` and `A → C` can fire in any order once `A` is established. Implication naturally forms trees and linked lists: `A → B → C → D` is a chain, multiple implications with the same consequent form a choice, and multiple implications from the same antecedent form a fan-out.

Crucially, equivalence (`A ↔ B`) adds nothing new: it is just `A → B` and `B → A` together, which lets you derive each from the other. The interesting structure is always the directed part.

What makes implication-based computation powerful is compositionality. Adding a new rule `E → F` to a program cannot break existing derivations — it can only add new ones. The closed-world assumption can be relaxed: you can extend the language of a domain without renegotiating the entire system. This is the promise of declarative programming that sequential code never quite delivers.

---

## What We Are Building

Evident is not trying to be Prolog with cleaner syntax. It is trying to be a language where:

- The programmer specifies *what is sufficient* to establish a claim, not *how* to compute it
- The structure of evidence is as visible and manipulable as the structure of data
- Order-independence is the default, not an optimization
- Decomposing a high-level name into sub-claims is the primary design act
- The runtime can find a valid evaluation order rather than the programmer having to specify one

The closest existing systems are proof assistants (Coq, Lean) and Answer Set Programming (Clingo) — but neither is designed as a general-purpose programming language. Evident is meant to fill that space: the expressiveness of logic programming, the evidence model of type theory, and the execution flexibility of dataflow systems, packaged as something programmers can actually use.
