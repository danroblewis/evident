# Implication vs. Function Calling: What Makes Evident Different

The uncomfortable truth: under the Curry-Howard correspondence, intuitionistic implication *is* the function type. They are literally the same thing in constructive logic. This is why every attempt to build Evident winds up looking like functional programming. It is not a syntax failure — it is a philosophical near-identity.

But "near" is doing a lot of work in that sentence. This document maps the territory between functional programming, Prolog, and what Evident is actually trying to be — and identifies exactly where the differences live.

---

## Why the Syntax Always Converges

Functional programs and Prolog programs look alike for a reason that goes deeper than stylistic preference. Both are systems for reasoning about **inductively structured tree-shaped data**. A list is either empty or a head plus a tail. A number is either zero or a successor. Any system that reasons about such structures will independently discover the same notation, because the notation *is the shape of the data*.

```haskell
-- Haskell
length []     = 0
length (_:xs) = 1 + length xs
```

```prolog
% Prolog
length([], 0).
length([_|T], N) :- length(T, N1), N is N1 + 1.
```

These are not two languages that happened to look similar. They are two different semantic systems that independently encoded the same mathematical structure — the inductive definition of a list — and both are using `name(args)` notation because that is how you name a relationship between things. Whether that relationship is called a function, a predicate, or a claim, the notation converges.

The implication is uncomfortable: **you cannot make Evident look different from FP simply by choosing a different notation for the same underlying concept**. If Evident's claims are isomorphic to functions, they will look like functions no matter what you call them. The differentiation must be semantic, and then the syntax can signal it.

---

## What Functions Are

A function in the mathematical sense is a *total, deterministic map* from input to output. Calling a function is *evaluating an expression to a value*. The function is the map; the call is following the map.

```haskell
factorial 0 = 1
factorial n = n * factorial (n - 1)
```

`factorial 5` evaluates to `120`. There is one answer. There is no search. The computation terminates at a value, and the value *is* the entire meaning of the call. Referential transparency follows: `factorial 5` can be replaced by `120` anywhere without changing the program's meaning.

Functions can be passed to other functions, returned from functions, stored in data structures. They are values. Composition is function application: `(f . g)(x) = f(g(x))`.

**The unit of meaning in FP is the function application, and the function application *produces* a value.**

---

## What Prolog Relations Are

Prolog predicates are not functions. They express *relations* — and "calling" a predicate means initiating a proof search.

```prolog
sorted([]).
sorted([_]).
sorted([A, B | Rest]) :- A =< B, sorted([B | Rest]).
```

`sorted([1,2,3])` does not return `true`. It initiates SLD resolution: Prolog tries to *prove* that `sorted([1,2,3])` holds by finding a sequence of clause applications that leads to a successful derivation. If asked `sorted(?X)` with an uninstantiated variable, Prolog will enumerate all `X` satisfying the relation.

The central mechanism is **unification**: bidirectional pattern matching. `f(X, 3) = f(2, Y)` finds `{X=2, Y=3}` — both sides become equal under that substitution. A Prolog variable is not a storage location. It is a logical unknown in an equation system.

This is the real difference from FP: the **logical variable**. An uninstantiated Prolog variable can flow through multiple computations before being resolved. `X = f(Y), X = f(3)` unifies `f(Y)` with `f(3)`, binding `Y=3` by propagation. No evaluation occurred; the solver found consistent bindings.

**The unit of meaning in Prolog is the proof search, and the proof search *establishes* (or fails to establish) that a relation holds.**

---

## Where Prolog and FP Actually Differ

The Mercury project revealed something important: when you add *mode declarations* to Prolog — explicitly marking which arguments are inputs and which are outputs — most Prolog predicates turn out to be functions in disguise.

```mercury
:- pred sorted(list(int)).
:- mode sorted(in) is semidet.   % "is this list sorted?" — a partial function
```

In `in` mode, `sorted` is a partial function: given a list, either it succeeds (sorted) or fails (not sorted). It is semantically identical to a boolean function.

What Mercury reveals is that **the Prolog/FP distinction is a spectrum controlled by one variable: the logical variable**. When all arguments are ground (fully instantiated), Prolog predicates are functions. When some arguments are uninstantiated, you get genuine relational computation — bidirectionality, non-determinism, constraint propagation. The "relational-ness" lives entirely in the uninstantiated variables.

The table:

| | Functional | Prolog (grounded) | Prolog (with logic vars) |
|---|---|---|---|
| Evaluation model | Expression → value | Proof search | Constraint solving |
| Multiple solutions | No (one value) | Simulate with lists | Yes, native |
| Bidirectionality | No | No | Sometimes |
| Variables | Names for values | Names for values | Unknown quantities |
| Composition | Function application | Conjunction of goals | Conjunction with unification |

---

## The Curry-Howard Trap

Here is the uncomfortable part. The Curry-Howard correspondence says:

- A proof of `A → B` in intuitionistic logic *is* a function from proofs of `A` to proofs of `B`
- A proof of `A ∧ B` *is* a pair of proofs
- A proof of `A ∨ B` *is* a tagged choice of proof

Under this isomorphism, propositions are types and proofs are programs. **Intuitionistic implication and the function type are literally the same thing.** In Agda or Coq, `A → B` is simultaneously the type of functions from A to B and the type of proofs that A implies B. There is no distinction.

This is why Evident keeps looking like a dependently-typed functional language. If you build a language where:
- Claims correspond to propositions
- Evidence corresponds to proof terms
- Decomposition rules correspond to implications

...then you have built exactly what Agda already is. The semantics are isomorphic. The syntax will converge.

**So what is Evident actually trying to be, and how is it different?**

---

## Where Implication Is Not Function Application

Several flavors of implication *are* genuinely different from function application. These are the design spaces Evident can occupy.

### 1. Multiple Independent Warrants

In FP, a function maps each input to exactly one output. Overloaded functions (or typeclasses) dispatch to one implementation based on type — still one output per input.

In Evident, a claim can be established by multiple *independent* sufficient conditions:

```evident
evident payment_authorized because { card_valid, funds_sufficient }
evident payment_authorized because { bank_transfer_confirmed }
evident payment_authorized because { voucher_redeemed }
```

These are not cases of the same function — they are three separate warrants, any of which is sufficient. The claim `payment_authorized` is established when *any one* of these holds. In FP, you would need explicit disjunction: `Either CardValid BankTransfer Voucher → PaymentAuthorized`. The disjunction is manual; the programmer must construct it. In Evident, the multiple decompositions *are* the disjunction, and the runtime handles the choice without the programmer specifying how.

This is the **epistemic warrant** reading: "card_valid is sufficient reason to believe payment_authorized." It is not "card_valid is an input that computes payment_authorized." The warrant can come from any direction. Multiple warrants for the same conclusion are a first-class concept.

### 2. The Evidence Base is Shared, Not Scoped

In FP, function calls are scoped: `f(x)` has access to `x` and whatever `f` closes over. There is no shared global "what we know" state (without explicit threading or monads).

Evident maintains a **shared, growing evidence base**. When `card_valid` is established, it is established for everyone — it does not need to be re-derived for each downstream claim that depends on it. This is the Datalog/fixpoint model: claims are facts in a database, not computations that execute per-call.

```evident
-- Once established, card_valid is available to all claims that need it
evident card_valid because { luhn_check_passes, expiry_in_future }
evident payment_authorized because { card_valid, funds_sufficient }
evident fraud_check_cleared because { card_valid, merchant_known }
```

Both `payment_authorized` and `fraud_check_cleared` consume `card_valid` from the evidence base. In FP, you would call `validateCard(card)` twice — or thread the result explicitly. In Evident, the evidence base makes the fact available everywhere once, at no extra cost.

### 3. Order Independence as a Semantic Property

In FP, the order of evaluation is determined by the expression structure. `f(g(x), h(x))` evaluates `g(x)` and `h(x)` before `f`, in whatever order the language specifies. Changing the order may change the result (for impure functions) or violate strictness assumptions.

In Evident, **the order of claim establishment is not a semantic property**. `evident A because { B, C }` means B and C must both be established; the runtime can establish them in any order, in parallel, or interleaved. The claim A is the same regardless.

This is not just an optimization. It reflects the epistemic reading: gathering evidence is not a procedure that must be followed in sequence. You accumulate reasons; once enough reasons are gathered, the claim is warranted. The order of accumulation is irrelevant.

### 4. Querying vs. Calling

In FP, `factorial(5)` is a computation — you issue a command and get a result. In Evident, `? factorial(5, ?f)` is a query — you ask whether there exists an `f` such that the claim `factorial(5, f)` is evident, and if so, what it is.

The distinction is subtle but deep. A command presupposes that the answer will be computed. A query presupposes that the answer already exists in the evidence base (or can be derived from it), and you are asking to have it surfaced.

This is why Evident uses `?` for queries rather than function-call syntax. The `?` signals: "I am asking the evidence base a question, not commanding a computation."

### 5. Evidence as Residue, Not Result

In FP, `factorial(5)` returns `120`. The return value is the output. Nothing else persists.

In Evident, establishing `factorial(5, 120)` leaves behind an **evidence term** — the derivation tree showing how this was established. This residue is a first-class value that can be inspected, logged, stored, and passed to other claims.

```evident
? factorial(5, ?f) as ev
-- ev = FactStep(5, FactStep(4, FactStep(3, FactStep(2, FactStep(1, FactBase)))))
```

In FP, the computation evaporates. In Evident, the derivation persists. This is the difference between a function that returns a value and a rule system that accumulates knowledge.

---

## The Actual Difference from Prolog

Prolog's surface similarity to FP is misleading. The real Prolog is relational (logical variables, unification, non-determinism). But in practice, Prolog is written procedurally:

- Clause ordering is semantically significant (different orderings produce different results)
- The `cut` operator commits to a branch and eliminates backtracking
- Programmers reason about execution order constantly

Evident differs from Prolog in the following ways:

| | Prolog | Evident |
|---|---|---|
| Clause ordering | Semantically significant | Irrelevant |
| Multiple solutions | Via backtracking (ordered) | Via independent warrants (unordered) |
| Evidence terms | Discarded after success | First-class persistent values |
| Execution model | SLD resolution (depth-first) | Fixpoint over evidence base |
| Negation | Negation-as-failure (ordered) | Stratified or explicit |
| Bidirectionality | Sometimes (for grounded predicates) | Not a goal; evidence is directional |

The deepest difference: **Prolog is a proof-search procedure; Evident is a knowledge accumulation process**. In Prolog, you issue a query and the system searches for a proof. In Evident, the system continuously accumulates evidence as new facts are asserted, and you query the accumulated state.

---

## How Syntax Can Signal the Semantic Difference

Given all of the above, what can Evident's syntax do to make the semantic difference legible?

**The `evident` keyword**: marks a claim declaration. Not a function definition. Not a predicate. A claim that can be established.

**`?` for queries**: marks a question to the evidence base, not a function call. `? sorted([1,2,3])` asks whether the claim is established. `factorial(5)` in FP computes a value.

**`because` / indentation body**: marks warrant conditions, not arguments. The body of a claim is the conditions under which it is warranted, not the instructions for computing it.

**Multiple `evident` declarations for the same claim name**: not overloading (FP), not alternative clauses (Prolog's ordered search), but independent warrants. Any one suffices.

**`=>` for forward implication**: when `A` is established, `B` becomes established. This is not function composition (`f . g`). It is warrant propagation: A's establishment is a reason for B's establishment.

**Evidence binding**: `? A as ev` — you query the claim AND bind the evidence term. In FP, you call a function and get a return value. In Evident, you ask a question and get both a yes/no and the reason.

---

## Summary

| | Functional | Prolog | Evident |
|---|---|---|---|
| Primary unit | Function (maps input to output) | Relation (holds or not for argument tuples) | Claim (established or not in the evidence base) |
| "Calling" | Evaluate an expression | Initiate a proof search | Query the evidence base |
| Multiple solutions | No | Via backtracking (ordered) | Via independent warrants (unordered) |
| What persists | Return value | Bindings | Evidence terms in shared base |
| Order | Evaluation order is structural | Clause order is semantic | No order; fixpoint semantics |
| Variables | Names for values | Logical unknowns | Universally quantified in heads, existential in bodies |
| Negation | Not applicable (total) | Negation-as-failure | Stratified; absence of evidence |
| Composition | Function application | Goal conjunction | Warrant accumulation |

The reason these languages keep converging syntactically is that `name(args)` is the natural notation for "X stands in the Y-relation to Z," and all three are doing exactly that. What differs is what *kind* of relation is named: a map (FP), a provable proposition (Prolog), or an established warrant (Evident).

The syntax makes the distinction legible through keywords (`evident`, `?`, `because`, `=>`), through the evidence binding mechanism (claims have persistent evidence terms, not just return values), and through the absence of ordering — multiple `evident` declarations for the same name are independent alternatives, not cases in a match expression and not ordered alternatives in a search tree.

Evident is to Prolog what Prolog promised to be: a language where you say what is true, and the system finds out whether it is. The difference is that Evident keeps the promise.
