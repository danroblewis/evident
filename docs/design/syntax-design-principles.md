# Syntax Design Principles for Evident

Five parallel research threads — terse notation (APL/J/K), syntax design patterns, cognitive science, declarative language syntax, and PL history — converge on a set of principles directly applicable to Evident's design. This document synthesizes the findings and draws concrete implications.

---

## Executive Summary

1. **Notation shapes thought, not just representation.** Before asking "what should this look like?", ask "what thoughts does this notation make easy, and what thoughts does it make hard?" (Iverson, 1979)
2. **Code comprehension uses formal-logic circuits, not language circuits.** Neuroscience evidence (Ivanova et al., *eLife* 2020) shows programming activates the multiple-demand network, not Broca's area. English-like syntax provides no cognitive benefit and may actively mislead.
3. **The `where` pattern is the right mental model for Evident.** State the high-level claim first; decompose below. This is how humans explain, and it is what Haskell, Mercury, and Agda all converged on independently.
4. **Named intermediate results are not optional.** Working memory fails on chains longer than 2–3 steps. Named sub-claims are chunking handles, not bureaucracy.

---

## What We Learned

### Iverson's Criterion: Notation as a Tool of Thought

Iverson's 1979 Turing lecture identified five properties a good notation must have: ease of expression, **suggestivity** (the form in one domain hints at related operations in others), ability to subordinate detail, economy, and amenability to proof. APL was designed to satisfy all five simultaneously.

The most actionable property for Evident is suggestivity. A symbol for "A makes B evident" should suggest the transitivity chain: if A makes B evident and B makes C evident, then A makes C evident. The notation should make this compositionality visible, not buried.

The secondary lesson is the distinction between structural density and arbitrary density. A symbol earns its place if it encodes a **composable, regular operation recoverable by rule**. It does not earn its place if it merely abbreviates. APL's `⍤` (rank operator) is structural — its meaning follows from a consistent overloading rule across the array algebra. `cn` for `connection` is arbitrary — it requires a lookup. Evident's symbols must pass this test.

The calculus notation war between Newton and Leibniz is the sharpest historical proof. Leibniz's `dy/dx` won because it makes the chain rule look like fraction cancellation: `(dy/dt) = (dy/dx)(dx/dt)`. Newton's ẋ hid the variable of differentiation. The notation that encodes the structural rule beats the notation that doesn't, regardless of which came first.

---

### The Cognitive Science Picture

Three empirical findings bear directly on Evident:

**Code comprehension uses the logic network, not the language network.** Ivanova et al. (2020, *eLife*) and Pattamadilok et al. found that code comprehension activates the fronto-parietal multiple-demand network (associated with formal reasoning) but not the language areas (Broca's area, Wernicke's area). Programming is cognitively closer to mathematics than to speech. The implication: making syntax "read like English" routes processing through the wrong substrate and provides no benefit. Evident should look like logic.

**Chunking requires canonicity.** Expert programmers perceive familiar syntactic patterns — loops, pattern matches, list comprehensions — as single cognitive units (Hermans, *The Programmer's Brain*, 2021). This requires consistency: if the same construct can be written three ways, experts cannot form stable chunks. One canonical form per construct, always.

**Reading dominates writing 10:1.** Time-tracking studies consistently find developers spend an order of magnitude more time reading code than writing it. Syntax should be optimized for the reader. Dense write-once notation (APL, Perl) is appropriate for expert-only, single-author, single-domain tools. A programming language with multiple contributors over time should pay the reading cost willingly.

**The named-chain principle.** Human reasoning accuracy over inference chains drops sharply after 2–3 anonymous steps (Evans, Newstead, Byrne 1993). Named intermediate results act as chunking re-entry points: the reader can stop at a named sub-claim, verify it independently, and continue. Deep unnamed nesting is a cognitive bottleneck. In Evident, every sub-claim should have a name.

---

### The `where` Pattern Is Already Right

The `where` construct appears independently in Haskell, Mercury, Agda, and Evident's existing design. The structure is invariant:

```
main claim
  where
    sub-claim-1 = ...
    sub-claim-2 = ...
```

This matches human explanation order: state the conclusion, then justify it. It is also the top-down design order: name what you want, then figure out what that requires. Evident's `because { }` block is this pattern exactly. What changes with better syntax design is removing the explicit delimiters via a layout rule (below) and potentially renaming the keyword — but the structure is correct.

---

### Agda's Mixfix: The Right Extensibility Model

Agda allows user-defined operators by placing `_` in identifier names. Defining `if_then_else_ : Bool → A → A → A` makes `if b then x else y` valid syntax immediately, with no parser modification. Underscores mark argument positions; precedence is declared separately. This is the most principled operator extension mechanism in the survey: the parser generically handles any sequence of identifier tokens with `_` as a mixfix operator; new operators install by declaration; no grammar production needs modification.

For Evident, this matters because domain-specific operators are inevitable. A database programmer wants `_has_index_on_`, a business analyst wants `_requires_approval_from_`. Agda's mechanism lets each declare their domain vocabulary without modifying a shared grammar, without resorting to the fragile Prolog approach (dynamic `op/3` calls that make the grammar context-sensitive).

---

### Layout Rules Eliminate Delimiter Noise

Haskell's layout rule (the "off-side rule") is a lexer transformation: indentation is converted to virtual braces and semicolons before the parser sees the token stream. The grammar itself still uses explicit delimiters; the layout rule is a surface-level convenience that eliminates visual noise without changing semantics.

Scala 3 adopted optional indentation syntax (replacing `{}`  with indentation), reducing program length by over 10% across the corpus. The tradeoff — fragility around tabs vs. spaces, difficulty for code generators — is real but acceptable when the layout rule is treated as the canonical style.

Evident's `because { B, C, D }` should become:

```evident
evident A
    B
    C
    D
```

The `because` keyword is optional filler once the layout rule establishes the structural meaning of indentation under a claim head. The `{` `}` become noise.

---

### Lean's `by`: Bridging Tactic and Term Modes

Lean 4 uses `by` to switch between term mode (write the proof term directly) and tactic mode (issue proof-construction strategies). Both produce the same underlying elaborated term. The programmer picks whichever is clearer at the abstraction level they are working at.

Evident's `because` block is already tactic mode — "to establish A, establish B, C, and D." The evidence term is implicit. Term mode would allow writing evidence explicitly:

```evident
evident sorted([1, 2, 3]) by
    SortedCons(le(1,2), SortedCons(le(2,3), SortedOne))
```

The `by` keyword is the bridge. For most programming, the tactic-style `because` block is right. For domain-specific code where the structure of evidence matters — audit trails, proof logging, certified computation — term mode is the right expression.

---

### Mercury's Determinism Annotations

Mercury requires every predicate to declare its determinism: `det` (exactly one solution), `semidet` (zero or one), `nondet` (zero or more), `multi` (one or more). The compiler verifies this statically.

For Evident, this annotation belongs on claim declarations, not bodies:

```evident
claim factorial : Nat -> Nat -> det        -- exactly one result
claim member    : Nat -> List Nat -> semidet  -- it either is or isn't
claim path      : Node -> Node -> nondet   -- multiple paths may exist
```

This helps the runtime (deterministic claims can short-circuit search; nondeterministic ones need full exploration), helps the programmer (intent is documented), and helps the type-checker (generating evidence values of the right shape).

---

### The Mutual Exclusion Problem

This is the hardest unsolved syntax problem for Evident, and the research explains why it keeps appearing in all the syntax explorations.

In Prolog, mutual exclusion between cases is implicit in clause ordering — once the first matching clause succeeds, alternatives are not tried. Prolog programmers rely on this silently. In Evident, clause ordering is irrelevant by design, so mutual exclusion must be explicit. The four options:

| Approach | Example | Assessment |
|---|---|---|
| Guards on claim heads | `evident fizzbuzz(n, "FizzBuzz") when n % 15 == 0` | Best: local, explicit, readable |
| `otherwise` fallthrough | `evident fizzbuzz(n, s) otherwise` | Good for catch-all cases |
| Explicit negation | `not divisible_by(n, 3)` in body | Correct but verbose |
| Disjoint case block | `cases fizzbuzz(n, r) { ... }` | Bundled alternatives, high ceremony |

Guards are the right primary mechanism. They match Haskell's `|` guard syntax, which has decades of evidence of being readable and learnable. The Haskell pattern:

```haskell
fizzbuzz n
    | n `mod` 15 == 0 = "FizzBuzz"
    | n `mod` 3  == 0 = "Fizz"
    | n `mod` 5  == 0 = "Buzz"
    | otherwise        = show n
```

...is the template. `otherwise` is a guard that always matches, handling the catch-all case cleanly.

---

## Synthesis: What to Keep, Change, Add, and Avoid

### Keep
- `evident` as the primary keyword — correct, meaningful, unique
- Parameterized claims — essential
- Self-evident base cases (claim with no body) — correct
- Top-down decomposition as the primary design act

### Change
- Replace `because { }` with layout rule (indentation-based bodies)
- Make `because` optional — the indentation already signals decomposition
- Replace comma-separated sub-claims with newline-separated (once layout rule is in place)

### Add
- **Guards**: `when condition` on claim heads for mutual exclusion and case analysis
- **Determinism annotations**: `det`, `semidet`, `nondet` on claim declarations (optional)
- **`by` keyword**: bridge from tactic-style decomposition to explicit evidence terms
- **`?` prefix for queries**: `? sorted([1,2,3])`, `? factorial(5, ?f)` — idiomatic, terse
- **`=>` for forward implication**: `A => B` as a forward-chaining complement to backward decomposition
- **Mixfix operator declarations**: `_implies_`, `_requires_` installable by users

### Avoid
- English prose syntax — routes comprehension through the wrong cognitive substrate
- Delimiter-heavy notation without layout rule
- Anonymous deep nesting — forces token-by-token parsing even for experts
- Arbitrary symbol abbreviations — only symbols with prior established meaning (`→`, `∧`, `⊢`, `≤`)
- Multiple equivalent forms for the same construct — breaks chunking

### Why the Previous Explorations Were Wrong

**YAML/data-first**: Data formats have no operator precedence, no layout rule, no macro system, no ability to define new notation. What they gain (serializability) should be achieved at the IR level, not the surface level. A program is not a configuration file.

**English DSL**: Natural language proximity provides no cognitive benefit (neuroscience evidence) and creates a false promise that the language can be read without learning it. SQL is the exception — it works because relational algebra maps unusually cleanly onto natural language quantifiers. Logic and evidence-based reasoning does not have this property.

**Python decorator pattern**: Embedding logic in a host language's decorator system accepts the host language's syntax ceiling and makes the semantics of `@evident` permanently less clean than a first-class `evident` keyword. It is a library, not a language.

---

## A Candidate Syntax Direction

Synthesizing these principles, the next iteration of Evident uses:
1. Significant indentation (layout rule)
2. Guards (`when`) on claim heads for mutual exclusion
3. `?` prefix for queries, with `?var` for output binding
4. Optional determinism annotation
5. `by` for explicit evidence terms
6. `=>` for forward implication

```evident
-- Claim declaration (determinism optional)
claim factorial : Nat -> Nat -> det
claim sorted    : List Nat -> det
claim reachable : Node -> Node -> nondet

-- Factorial
evident factorial(0, 1)

evident factorial(n, result) when n > 0
    m = n - 1
    factorial(m, k)
    result = n * k

-- Sorted list
evident sorted([])
evident sorted([_])
evident sorted([a, b | rest]) when a <= b
    sorted([b | rest])

-- FizzBuzz (guards handle mutual exclusion, otherwise catches the rest)
evident fizzbuzz(n, "FizzBuzz") when n % 15 == 0
evident fizzbuzz(n, "Fizz")    when n % 3  == 0
evident fizzbuzz(n, "Buzz")    when n % 5  == 0
evident fizzbuzz(n, s)         otherwise
    s = to_string(n)

-- Forward implication
card_valid => payment_authorized

-- HTTP validation (hierarchical decomposition)
evident valid_api_request(req)
    method_allowed(req)
    valid_auth(req)
    valid_content_type(req)

evident method_allowed(req) when req.method in [GET, POST, PUT, DELETE]

evident valid_auth(req)
    req.auth.scheme == "Bearer"
    token_not_expired(req.auth.token)
    token_signature_valid(req.auth.token)

-- Query
? valid_api_request(incoming_request)
? factorial(5, ?f)           -- output binding: find f
? reachable(london, ?city)   -- enumerate all reachable cities
```

This is shorter and more expressive than any of the eight previous explorations. The `when` guard does the work that Prolog's clause ordering did implicitly. The layout rule eliminates `{ }`. The query syntax is two characters (`?`). The claim hierarchy reads top-down, the way a requirements document reads, but executes as logic.
