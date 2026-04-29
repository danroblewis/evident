# Language Design

This document sketches the Evident language: its syntax, execution model, and core design choices. Everything here is exploratory — the goal is to make the ideas concrete enough to reason about, not to propose a final specification.

---

## Core Concepts

Evident has three primitive operations:

1. **Declaring** a claim as the kind of thing that can be evidenced
2. **Evidencing** a claim by decomposing it into sub-claims
3. **Asserting** a ground fact as axiomatically evident

And one primitive *query*:

4. **Asking** whether a claim is evident given the current knowledge base

---

## Syntax Sketch

### Claims and Decomposition

```evident
-- Declare that a claim is evident when certain sub-claims hold:
evident http_response_ok(response) because {
    response.status == 200,
    response.body is_not_empty
}

-- Multiple decompositions are alternatives (any one suffices):
evident http_response_ok(response) because {
    response.status == 201,
    response.location is_present
}

-- A claim with no body is self-evident (an axiom):
evident empty_list([])

-- A parameterized claim:
evident sorted([])
evident sorted([_])
evident sorted([a, b | rest]) because {
    a <= b,
    sorted([b | rest])
}
```

### Implication

```evident
-- Direct implication: if A is evident, then B is evident
payment_processor_live => system_can_charge

-- Implication with parameters:
valid_token(t) => user_is_authenticated(t.user_id)

-- A chain: if A then B, if B then C
card_details_valid(card) => payment_authorized(card)
payment_authorized(card) => order_can_be_placed(card)
```

### Asserting Ground Facts

```evident
-- Assert that a concrete fact holds (no decomposition needed):
assert card_details_valid(card { number: "4111...", expiry: "12/28", cvv: "123" })

-- Assert from external input:
assert request(http_request from stdin)
```

### Querying

```evident
-- Ask whether a claim can be established:
? http_response_ok(response)

-- Ask for the evidence too:
? http_response_ok(response) with evidence

-- Ask and bind the evidence term to a name:
? sorted(list) as ev

-- Ask for all parameter bindings that make a claim evident:
? payment_authorized(card) for card
```

---

## Evidence Terms

When a claim is established, the runtime produces an **evidence term** — a structured value capturing the derivation. Evidence terms are first-class: you can inspect them, store them, pass them to other claims, and pattern-match on them.

```evident
-- Establish sorted([1, 2, 3]) and inspect the evidence:
? sorted([1, 2, 3]) as ev

-- ev might look like:
SortedList {
    head: 1,
    tail_evidence: SortedList {
        head: 2,
        tail_evidence: SortedList {
            head: 3,
            tail_evidence: SingleElement(3)
        },
        comparison: LessOrEqual(2, 3)
    },
    comparison: LessOrEqual(1, 2)
}
```

Evidence terms carry the *why*, not just the *that*. A claim established via different decompositions produces structurally different evidence — which may or may not matter depending on whether the application treats evidence as proof-relevant or proof-irrelevant.

---

## Example Programs

### Factorial

```evident
-- Base case: 0! = 1 is self-evident
evident factorial(0, 1)

-- Recursive case: n! = n × (n-1)! given (n-1)! = k
evident factorial(n, result) because {
    n > 0,
    m = n - 1,
    factorial(m, k),
    result = n * k
}
```

This is syntactically similar to Prolog, but the clauses are unordered. The runtime may try both in any order; the base case will succeed immediately for `n = 0` and the recursive case will be tried for positive `n`. No explicit ordering is needed because the `n > 0` guard makes the cases mutually exclusive.

### List Membership

```evident
evident member(x, [x | _])           -- x is a member of any list starting with x

evident member(x, [_ | rest]) because {
    member(x, rest)                   -- x is a member if it's in the tail
}
```

Again, the two clauses are unordered. The runtime can try them in any order; the first will match when `x` is the head, the second when it isn't. The order doesn't matter because the runtime's job is to find *a* derivation, not to follow a prescribed search path.

### HTTP Request Validation

```evident
evident valid_api_request(req) because {
    req.method in [GET, POST, PUT, DELETE],
    valid_api_key(req.headers.authorization),
    valid_content_type(req.headers.content_type)
}

evident valid_api_key(auth) because {
    auth.scheme == "Bearer",
    token_not_expired(auth.token),
    token_signature_valid(auth.token)
}

-- Self-evident predicates (checked directly):
evident token_not_expired(token)   -- checked against current time
evident token_signature_valid(token) -- checked against key store

-- Usage:
? valid_api_request(incoming_request) as evidence
  then serve_response
  else reject_with(403, evidence)
```

The `reject_with(403, evidence)` branch is interesting: when the request fails validation, you pass the *evidence of failure* to the error handler. The error handler knows exactly which sub-claim failed and why — not just that validation failed.

### Graph Reachability

```evident
-- Direct edge:
evident reachable(a, b) because {
    edge(a, b)
}

-- Transitive:
evident reachable(a, c) because {
    reachable(a, b),
    reachable(b, c)
}

-- Assert the graph:
assert edge(london, paris)
assert edge(paris, berlin)
assert edge(berlin, warsaw)

-- Query:
? reachable(london, warsaw)    -- evident via chain
? reachable(warsaw, london)    -- not evident (no back edges)
```

---

## Execution Model

The Evident runtime maintains:
- A **fact base**: the set of currently established claims and their evidence
- A **rule base**: the set of decomposition rules and implications
- A **work queue**: claims that have been asserted but not yet fully evidenced

Evaluation is a fixpoint computation:
1. Start with asserted ground facts
2. For each claim in the work queue, try all applicable decomposition rules
3. A decomposition succeeds when all sub-claims are established
4. When a decomposition succeeds, add the claim to the fact base and remove it from the queue
5. When the fact base grows, propagate: find all rules whose antecedents are now fully established and add their consequents to the work queue
6. Repeat until the work queue is empty (fixpoint reached)

The runtime may execute steps in any order consistent with the dependency graph. Independent sub-claims may be evaluated in parallel. The programmer has no visibility into — and no control over — the evaluation order, unless they declare an explicit dependency.

### Comparison with Prolog Execution

| | Prolog | Evident |
|---|---|---|
| Strategy | Depth-first, left-to-right | Any valid topological order |
| Backtracking | Implicit, global | Per-claim, local |
| Evidence | Discarded after success | First-class value |
| Clause ordering | Semantically meaningful | Irrelevant |
| Loops | Possible on recursive rules | Require explicit well-founded ordering |
| Parallelism | Requires annotation (parallel Prolog) | Default for independent claims |

---

## Implication Chains as Program Architecture

A practical Evident program is a hierarchy of claims, where high-level claims decompose into mid-level claims, which decompose into ground-checkable leaves.

```
order_can_be_placed
    ├── payment_authorized
    │   ├── card_valid
    │   │   ├── number_passes_luhn
    │   │   └── expiry_in_future
    │   └── funds_sufficient
    │       └── (external: bank API)
    └── inventory_available
        ├── item_in_stock
        └── warehouse_can_ship_today
```

Each name in this tree is a claim. Each claim is established by establishing its sub-claims. The root is established when the leaves are established. This is a dependency tree, automatically derived from the decomposition rules — the programmer never builds the tree explicitly.

---

## What Makes a Claim "Self-Evident"

Leaves of the dependency tree must be self-evident — established without further decomposition. What qualifies?

1. **Definitionally reducible expressions**: `2 + 2 == 4` — both sides reduce to `4` by the semantics of addition
2. **Externally asserted facts**: `assert edge(london, paris)` — the programmer declares this to be true
3. **Primitive predicates**: type checks, comparisons, membership tests that the runtime can check directly
4. **External oracles**: calls to external APIs, databases, or sensors that return a definite answer

The boundary between "self-evident" and "requires decomposition" is the boundary between the language's built-in reasoning and the programmer's specification. Self-evident claims are the language's axiom schema; everything above them is the programmer's logic.

---

## Negation and Absence of Evidence

Evident's default is the closed-world assumption: if a claim cannot be established from the current rule and fact base, it is treated as not evident. This is not the same as being *false* — it is being *unestablished*.

A future design question: should Evident support classical negation (explicitly asserting that something is false) alongside absence-of-evidence negation? Answer Set Programming supports both; Prolog supports only negation-as-failure. The two interact in subtle ways with non-monotonic updates.

For now, the simplest approach: `not A` is evident if and only if `A` is not evident in the current fact base, and the evaluation has reached a fixpoint (no more rules can fire). This is Datalog-style stratified negation.

---

## Syntax Alternatives Considered

The `because` keyword makes the top-down decomposition reading explicit: "A is evident because these sub-claims are evident." Alternative keywords that have been considered:

- `when` — `evident A when { B, C }`: conditional reading
- `given` — `evident A given { B, C }`: hypothetical reading
- `supported_by` — verbose but precise
- `←` — the Prolog syntax, familiar to logic programmers
- `from` — `evident A from { B, C }`: derivation reading

The `because` keyword is preferred because it matches the user-facing metaphor: when someone asks "why is A evident?", you show them the `because` clause. The answer to "why?" is the evidence.
