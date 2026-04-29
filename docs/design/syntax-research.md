# Syntax Research: Declarative, Functional, and Logic Languages

*How existing languages handle syntax for logical relationships and constructive proofs, and what it means for Evident.*

---

## Haskell: Expressiveness Through Layers

Haskell's syntax is additive: each mechanism targets a specific cognitive task, and they compose. The layout rule (significant indentation) eliminates delimiter noise in `where` and `let` blocks. `where` itself is the defining pattern — the main claim comes first, decomposition follows:

```haskell
sortedInsert :: Ord a => a -> [a] -> [a]
sortedInsert x [] = [x]
sortedInsert x (y:ys)
  | x <= y    = x : y : ys
  | otherwise = y : sortedInsert x ys
```

Guards (`| condition = expr`) are Haskell's inline case analysis for logical branches; they feel like mathematical case-by-case definitions. Point-free style and `.` (composition) let you write transformation pipelines as function algebra rather than data flow, which compresses a great deal of semantic content once you read it. `$` eliminates parenthesis nesting and makes the right-to-left application order explicit.

**What makes Haskell feel expressive to experts:** The combination of type classes (overloaded behavior with compile-time dispatch), do-notation (monadic sequencing that looks imperative), and layout sensitivity means that programs at different abstraction levels can look and feel natural. A proof-search monad written in Haskell with do-notation looks nearly identical to an I/O action. The syntax doesn't privilege any one abstraction.

**Key lesson for Evident:** The `where` pattern — state the main claim, then decompose below it — is the natural reading order for logic. Haskell's layout rule enforces this without explicit delimiters. Evident's `because { ... }` block is doing the same structural work.

---

## Agda: Syntax as Language Extension

Agda's central syntactic innovation is **mixfix operators**: any identifier with `_` placeholders can be used as infix, prefix, postfix, or even circumfix syntax. `_+_`, `if_then_else_`, `⟨_,_⟩` are all user-defined, not built-in. This means the surface syntax is infinitely extensible without parser changes — you define your notation in the same module where you define your types.

Agda also commits fully to Unicode: `∀`, `→`, `⊢`, `≤`, `∧` are default notation for the concepts they denote mathematically. The rationale is that these symbols have well-established meanings with decades of literature; using ASCII approximations (`->`, `<=`) creates a dialect problem — the code looks like the math but isn't quite.

**Dependent type telescopes** in Agda write parameter dependencies explicitly:

```agda
sorted-merge : {A : Set} → (leq : A → A → Bool) → (xs ys : List A) → List A
```

The `{A : Set}` is an implicit argument (Agda infers it from context); the explicit `leq` is a value that subsequent parameters depend on. This is the telescope syntax — a sequence of binders where later ones can reference earlier ones.

The **`with` abstraction** lets you pattern-match on an intermediate computed value in a way that makes the match available in the type-level context:

```agda
filter p [] = []
filter p (x ∷ xs) with p x
... | true  = x ∷ filter p xs
... | false = filter p xs
```

The `...` continues the previous pattern, and the `with` introduces a new column of pattern scrutiny. This is the syntactic mechanism for the proof-theoretic "case split on a computed value" — it matters when the type of the result depends on what the case was.

**Key lesson for Evident:** Mixfix operators are one of the most powerful syntax-extension mechanisms ever designed. Agda's approach to Unicode reflects a genuine claim: if your language is doing mathematics or logic, the notation should match the literature. Evident's target audience spans logic programmers (comfortable with `:- `) and type-theory practitioners (comfortable with `⊢`); the syntax choice is an audience choice.

---

## Lean 4: Closing the Gap Between Proof and Program

Lean 4 was designed simultaneously as a proof assistant and a practical programming language — the same language, not two embedded sub-languages. The primary mechanism bridging tactic proofs and term proofs is the `by` keyword:

```lean
theorem sorted_empty : Sorted [] := by
  constructor
```

`by` switches from term mode to tactic mode. Inside `by`, you describe how to construct the proof by issuing tactics (`constructor`, `apply`, `simp`, `omega`). Outside, you write the proof term directly. Both produce the same underlying elaborated term; the distinction is only in how you construct it.

Lean 4's **do-notation** works for any monad — it is syntax sugar for monadic bind, exactly as in Haskell, but Lean's type inference is powerful enough that the monad is usually inferred without annotation. This lets monadic code (I/O, state, failure, search) look like imperative code.

**What Lean 4 gets right on readability vs. precision:** Tactic blocks are readable to beginners (they look like instructions) but correspond to precise term constructions. The elaborated term is always inspectable with `#check` and `#print`. The key insight is that these two views — the tactic view and the term view — are interchangeable; choosing between them is a presentation choice, not a semantic one.

**Key lesson for Evident:** Evident's `because { B, C, D }` is tactic mode: you are describing a construction strategy. The evidence term produced is the elaborated term. Lean proves these can coexist in one language with `by`. A future Evident might support both a tactic-style block syntax and a term-mode syntax for writing explicit evidence, with the `by` bridge connecting them.

---

## Coq/Gallina: Notation as Infrastructure

Coq's **`Notation`** system is a macro system for the parser:

```coq
Notation "x + y" := (plus x y) (at level 50, left associativity).
```

This registers a rewrite rule for the parser: whenever it sees `x + y`, it produces the term `(plus x y)`. Notations can be scoped (active only in specific contexts), can bind operators at specified precedence levels, and can introduce multi-token patterns. The system is powerful but fragile — notation conflicts cause confusing parse errors, and heavy notation use can make error messages unreadable.

Coq's **`match ... with`** construct is the primary eliminator for inductive types and the primary proof mechanism for case analysis. In Coq, pattern matching is both a programming construct (for functions over data) and a proof construct (for case analysis over propositions). This unification comes from the Curry-Howard correspondence: a proof by cases is a function from disjunctions.

**Ltac**, Coq's tactic language, is a separate meta-language layered on top of Gallina. It is Turing-complete and can be used to write proof automation — but it is untyped and notoriously hard to debug. Lean 4 replaced Ltac with a typed tactic language that eliminates many of these issues.

**Coq vs. Agda on notation:** Agda's mixfix system is more principled — it operates on identifier tokens with `_` as holes, so extensions are always syntactic sugar for function application with no parsing ambiguity. Coq's notation system is more powerful but more dangerous — it can introduce arbitrary multi-token patterns, which can conflict unpredictably.

**Key lesson for Evident:** Coq's `match ... with` structure is the most natural encoding of "if A then B" at the type level — pattern matching on the evidence for A produces evidence for B. For Evident's evidence-as-data model, this suggests that consuming evidence from one claim to produce another is fundamentally a pattern-match operation.

---

## Prolog: Operators All the Way Down

Prolog's syntax is built on a single observation: **everything is a term**. The `:-` operator is not special syntax — `a :- b, c` is syntactic sugar for the term `':-'(a, ','(b, c))`. Operators are just terms with display conventions. This means Prolog's entire syntax can be extended with `op/3`:

```prolog
:- op(700, xfx, ===).
a === b :- a == b.
```

The `op/3` declaration specifies precedence, associativity (`xfx`, `xfy`, `yfx`, etc.), and the operator name. Because the entire language is just terms, user-defined operators integrate perfectly with the rest of syntax — they can appear in rule heads, rule bodies, assertions, and queries with no special treatment.

**Why this is powerful:** There is no distinction between "built-in operators" and "user operators" in Prolog at the syntactic level. `:- ` itself is defined this way. This makes Prolog the most extensible syntax of any language discussed here — you can create a DSL that looks nothing like Prolog but parses and executes as Prolog.

**Why Prolog feels different from Haskell:** Prolog's expressiveness is adversarial rather than compositional. Haskell's syntax is designed so mechanisms compose predictably (layouts, type classes, do-notation). Prolog's operator system is designed so anything can look like anything — which means you must read declarations to know what operators mean. The term-is-everything uniformity is elegant but makes programs harder to read without context.

**Key lesson for Evident:** Prolog's operator extensibility is the right aspiration but the wrong mechanism. Agda's mixfix system achieves similar extensibility with more predictable parse semantics. For a logic programmer encountering Evident, the `:-`-style syntax of the Prolog-adjacent exploration will feel familiar immediately; the question is whether the `evident` keyword and unordered-clause semantics are a small enough departure to feel natural.

---

## Mercury: Declaring What You Mean

Mercury imposes **mode declarations** on every predicate:

```mercury
:- mode sorted(in) is semidet.
:- mode sorted(out) is nondet.
```

A mode declaration specifies, for each argument, whether it is ground on entry (`in`) or uninstantiated (`out`), and the **determinism category**: `det` (always succeeds, exactly once), `semidet` (succeeds at most once), `nondet` (zero or more solutions), `multi` (one or more solutions). The compiler uses these declarations to verify that the clause body can be executed in the declared mode — effectively, it type-checks the control flow.

Syntactically, Mercury looks like Prolog with mandatory annotations. The declarations are verbose but serve a documentation function: reading a Mercury predicate header, you immediately know whether it is a function, a partial function, or a relation. In Prolog, this is only discoverable by reading the body or by convention.

**Key lesson for Evident:** Mercury's determinism annotations are the closest thing in logic programming to Evident's semantics. Evident claims that establish exactly one thing are deterministic; claims with multiple decompositions are nondeterministic in the evidence they produce. Explicit determinism annotations in Evident would help both the runtime (scheduling parallel vs. sequential evaluation) and the programmer (documenting expectations). This is the kind of annotation that belongs in the type signature rather than the body.

---

## miniKanren: The Embedding Tradeoff

miniKanren is a relational programming system designed to be embedded in a host language (Scheme, Clojure, Racket, JavaScript, Python). Its core operations — `fresh`, `==`, `conde` — are macros or functions in the host:

```scheme
(run* (q)
  (fresh (x y)
    (== q (list x y))
    (conde
      [(== x 1) (== y 2)]
      [(== x 3) (== y 4)])))
```

**What embedding gains:** you get the host language's entire infrastructure — data structures, I/O, modules, tooling — for free. miniKanren programs can call any host function; host programs can call miniKanren search. The combinatorial power of relational programming becomes available wherever you already have a host language.

**What embedding loses:** miniKanren cannot define new operators — it is limited to the host's syntax. `fresh`, `conde`, `==` look like special forms, but they are just functions with macro expansion. There is no way to make `sorted(x)` look like `sorted x` or to define `x is-sorted` as an operator. The gap between "writing miniKanren" and "writing natural logic" is fixed by the host language's expressiveness.

**Key lesson for Evident:** miniKanren's embedded design is the explicit trade-off Evident avoids. Evident is defined *as a language*, not embedded in another. This lets Evident control its own syntax, define what `because` and `evident` mean, and eventually support user-defined notation. The cost is that Evident must provide its own tooling infrastructure that an embedded DSL gets for free.

---

## Idris: Dependent Types Made Accessible

Idris was designed with the stated goal of being "Haskell for dependent types but accessible to Haskell programmers." Its key syntactic decision relative to Agda: **auto-implicits** and a cleaner telescope syntax.

In Agda, implicit arguments must be explicitly declared `{A : Set}`. In Idris, lowercase variable names in type signatures are automatically treated as universally-quantified implicit arguments:

```idris
sorted_merge : (leq : a -> a -> Bool) -> List a -> List a -> List a
```

Here `a` is automatically implicit. This dramatically reduces the boilerplate of writing polymorphic functions. The tradeoff: you lose explicit control over which variables are implicit, which can surprise users when a typo creates an unintended implicit.

Idris distinguishes **proof-relevant** and **proof-irrelevant** parts of a type explicitly. Terms in a `%World`-typed position are computationally relevant; propositions used only for constraints can be marked irrelevant with `0` (erased at runtime):

```idris
data Sorted : List Nat -> Type where
  SortedNil  : Sorted []
  SortedOne  : Sorted [x]
  SortedCons : (0 leq : x <= y) -> Sorted (y :: rest) -> Sorted (x :: y :: rest)
```

The `(0 leq : ...)` marks the ordering proof as erased — it is checked at compile time but not stored at runtime. This is the syntax-level distinction between "evidence that matters computationally" and "evidence that only matters for correctness."

**Key lesson for Evident:** Idris's erasure markers are exactly the syntax Evident needs for its proof-relevance open problem. Claims marked as proof-irrelevant can be established by any derivation (the runtime picks freely); proof-relevant claims carry their specific derivation tree. The syntactic mechanism — an annotation at the binding site — is cleaner than a global language-level policy.

---

## The `where` Pattern Across Languages

The `where` clause appears in Haskell, Mercury, Agda, and others as a mechanism for top-down exposition: state the main claim, then define its sub-components below it. This is pedagogically and cognitively natural — humans explain things top-down, deferring details. The main claim is the thing you want to be true; the `where` defines what makes it so.

In Haskell:
```haskell
isValidRequest req = methodOk && authOk && contentTypeOk
  where
    methodOk     = req.method `elem` [GET, POST, PUT, DELETE]
    authOk       = validateToken req.headers.authorization
    contentTypeOk = req.headers.contentType == "application/json"
```

In Mercury:
```mercury
is_valid_request(Req) :-
    method_ok(Req),
    auth_ok(Req),
    content_type_ok(Req).
```

The Mercury version has no `where`, but the same decomposition happens through named sub-goals defined elsewhere. The Haskell version explicitly defers the names.

Evident's `because { B, C, D }` is the `where`-pattern made structural: the body of a claim is the specification of what makes the claim true. The parallelism is direct — `evident A because { B, C }` is Haskell's `A = f where { B = ..., C = ... }` viewed from the conclusion, not the bindings.

---

## Cross-Cutting Observations

**What makes Haskell feel expressive vs. Prolog:** Haskell's expressiveness is compositional — each mechanism compounds with others predictably. Prolog's expressiveness is uniform — everything is a term — which is powerful but requires more context to read. Haskell programs can be read in layers: the type tells you what, the guards tell you when, the `where` tells you from what. Prolog programs require you to hold the clause order and cut semantics in your head.

**How Agda's mixfix enables new syntax without parser changes:** The `_` placeholder in an identifier signals a hole; the parser generically handles any sequence of identifier tokens with `_` as a mixfix operator. No grammar production needs to be modified; new operators are installed by declaration. This is the most principled operator extension mechanism across all these languages.

**What naturally encodes "if A then B":** In Prolog, `B :- A` (B is established if A is). In Haskell, a function `A -> B`. In Coq/Agda/Lean/Idris, `A -> B` as a type, with the function being the proof. In Evident, `A => B` or `evident B because { A }`. The logic-programming form (B if A) is top-down; the functional form (A implies B) is bottom-up. The top-down reading matches how humans state requirements ("payment is authorized when card is valid and funds are sufficient") which is why Evident's `because` reads more naturally to domain experts than a function arrow would.

**How these languages handle evidence / witness terms:** Agda and Lean represent evidence as typed terms in the same language as programs. Coq's Ltac keeps evidence construction separate from evidence terms. Prolog discards evidence after success. Haskell's type class dictionaries are hidden evidence terms managed by the compiler. Mercury doesn't expose evidence at all — determinism categories abstract over it. Of all these, only Agda, Lean, Coq, and Idris treat evidence as first-class values the programmer can inspect and pass. This is Evident's distinctive commitment: evidence is never discarded, always first-class, and always structured.

**What a logic programmer would find natural vs. surprising in Evident:** Natural: multiple clauses for the same claim (alternatives), unification-style variable binding, the `because` body as a conjunction of sub-goals, self-evident base cases. Surprising: unordered clauses (no cut, no committed choice), first-class evidence terms (Prolog discards them), the requirement to make cases explicitly mutually exclusive when ordering no longer enforces it, and fixpoint semantics rather than depth-first search. The mutual-exclusion point is the largest practical friction — Prolog programmers rely on ordering to avoid repeating conditions across clauses, and Evident's unordered model forces that work back to the programmer explicitly.
