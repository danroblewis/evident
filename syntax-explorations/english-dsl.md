# Syntax: English DSL

## Philosophy

The English DSL syntax treats programs as structured natural language: something a business analyst or domain expert could read aloud in a meeting and have it make sense. Claims read like policy statements or requirements. Conditions read like checklists. The goal is to minimize the gap between what the programmer writes and what they would say to explain the logic to a colleague. Symbols are used only when they carry unambiguous mathematical meaning (like `=`, `<=`, `*`). Everything else is spelled out. Evidence — the derivation of why a claim holds — is surfaced through `because` chains and `inspect` queries that mirror how you would ask "why?" in conversation.

---

## 1. Factorial

```evident
-- Factorial: establish that result is the factorial of n

claim factorial(n: 0, result: 1) is self-evident
  -- The factorial of zero is one, by definition.

claim factorial(n, result) is established when
  n > 0
  and n_minus_one = n - 1
  and factorial(n_minus_one, sub_result) is established
  and result = n * sub_result

-- Query: ask the runtime to establish factorial(5, answer)
-- and surface the evidence tree

establish factorial(n: 5, result: answer)
  and show evidence for answer

-- Expected output:
-- answer = 120
-- because factorial(5, 120) is established
--   because 5 > 0
--   and factorial(4, 24) is established
--     because 4 > 0
--     and factorial(3, 6) is established
--       because 3 > 0
--       and factorial(2, 2) is established
--         because 2 > 0
--         and factorial(1, 1) is established
--           because 1 > 0
--           and factorial(0, 1) is self-evident
```

**Assessment:** The base case reads naturally as a self-evident axiom. The recursive case flows well when read aloud: "factorial of n is established when n > 0, and factorial of n-1 is established, and result is n times that." The main awkwardness is the intermediate binding `n_minus_one = n - 1` — it breaks the flow with a side computation that feels more imperative than declarative. A future syntax might allow inline arithmetic in argument position: `factorial(n - 1, sub_result)`.

---

## 2. Sorted List

```evident
-- Sorted list: establish that a list is in non-decreasing order

claim sorted(list: []) is self-evident
  -- An empty list is trivially sorted.

claim sorted(list: [_]) is self-evident
  -- A single-element list is trivially sorted.
  -- The underscore matches any element without binding it.

claim sorted(list: [head, next, ...rest]) is established when
  head <= next
  and sorted([next, ...rest]) is established

-- Query: verify that a specific list is sorted

establish sorted(list: [1, 3, 3, 7, 12])
  and show evidence

-- Evidence would read:
-- sorted([1, 3, 3, 7, 12]) is established
--   because 1 <= 3
--   and sorted([3, 3, 7, 12]) is established
--     because 3 <= 3
--     and sorted([3, 7, 12]) is established
--       ...
--         and sorted([12]) is self-evident

-- Counterexample query:

establish sorted(list: [1, 5, 3])
  and show evidence

-- Evidence would read:
-- sorted([1, 5, 3]) cannot be established
--   attempted: head=1, next=5, rest=[3]
--     1 <= 5 holds
--     sorted([5, 3]) cannot be established
--       attempted: head=5, next=3, rest=[]
--         5 <= 3 does not hold
```

**Assessment:** The pattern-matching syntax in list heads (`[head, next, ...rest]`) is arguably the least "English" part of the file, but it is so widely recognized from modern languages that it earns its place. The failure evidence — showing what was attempted and where the chain broke — reads like a clear explanation of why the list is not sorted, which is exactly what you would want from a logic system. The `_` wildcard in the single-element case is a minor concession to programmer convention over pure English.

---

## 3. HTTP Request Validation

```evident
-- HTTP Request Validation
-- A request is valid when it passes method, auth, and content-type checks.

# Ground facts: the set of allowed HTTP methods

claim allowed_method("GET") is self-evident
claim allowed_method("POST") is self-evident
claim allowed_method("PUT") is self-evident
claim allowed_method("DELETE") is self-evident

# Auth token checks — each is a named sub-claim

claim valid_bearer_scheme(token) is established when
  token starts_with "Bearer "
  and token has_length_greater_than 7

claim not_expired(token) is established when
  expiry_of(token, expiry)
  and expiry > current_time()

claim valid_signature(token) is established when
  signature_of(token, sig)
  and signing_key_for(token, key)
  and hmac_matches(sig, key)

claim valid_auth_token(token) is established when
  valid_bearer_scheme(token) is established
  and not_expired(token) is established
  and valid_signature(token) is established

# Content type check

claim valid_content_type(request) is established when
  content_type_of(request, ct)
  and (ct = "application/json"
    or ct = "application/x-www-form-urlencoded"
    or ct starts_with "multipart/form-data")

# Top-level validation — all three sub-claims must hold

claim valid_api_request(request) is established when
  method_of(request, method)
  and allowed_method(method) is established
    provided because "method must be GET, POST, PUT, or DELETE"
  and auth_token_of(request, token)
  and valid_auth_token(token) is established
    provided because "token must use bearer scheme, be unexpired, and have valid signature"
  and valid_content_type(request) is established
    provided because "content type must be json, form-encoded, or multipart"

-- Query: validate a request and surface structured failure evidence

establish valid_api_request(request: incoming_request)
  and show evidence

-- If the token is expired, evidence reads:
-- valid_api_request(incoming_request) cannot be established
--   because valid_auth_token(token) cannot be established
--     because not_expired(token) cannot be established
--       because expiry_of(token, 1700000000)
--       and 1700000000 > current_time() does not hold
--         (current_time() = 1714000000)
--   note: "token must use bearer scheme, be unexpired, and have valid signature"
```

**Assessment:** The layered claim structure maps naturally onto how a security team would write validation rules in a requirements document. The `provided because` annotation is a readable way to attach human-readable failure labels without changing the logic. The built-in predicates (`starts_with`, `has_length_greater_than`) read like natural language but introduce an implicit standard library — the spec would need to define what predicates are available, which is a potential source of surprise for programmers.

---

## 4. Graph Reachability

```evident
-- Graph Reachability
-- Establish whether one node can be reached from another via directed edges.

# Ground facts: the edges of the graph

claim edge(from: "A", to: "B") is self-evident
claim edge(from: "B", to: "C") is self-evident
claim edge(from: "C", to: "D") is self-evident
claim edge(from: "A", to: "D") is self-evident
claim edge(from: "D", to: "E") is self-evident
claim edge(from: "X", to: "Y") is self-evident
  -- X and Y are in a disconnected component

# Direct reachability: one hop

claim reachable(from, to) is established when
  edge(from, to) is established

# Transitive reachability: via an intermediate node

claim reachable(from, to) is established when
  edge(from, via) is established
  and reachable(via, to) is established

-- Query: is E reachable from A?

establish reachable(from: "A", to: "E")
  and show evidence

-- Evidence reads:
-- reachable("A", "E") is established
--   because edge("A", "D") is self-evident
--   and reachable("D", "E") is established
--     because edge("D", "E") is self-evident

-- Alternative evidence (different derivation path):
-- reachable("A", "E") is established
--   because edge("A", "B") is self-evident
--   and reachable("B", "E") is established
--     because edge("B", "C") is self-evident
--     and reachable("C", "E") is established
--       because edge("C", "D") is self-evident
--       and reachable("D", "E") is established
--         because edge("D", "E") is self-evident

-- Query: is E reachable from X? (should fail)

establish reachable(from: "X", to: "E")
  and show evidence

-- Evidence reads:
-- reachable("X", "E") cannot be established
--   attempted via direct edge: edge("X", "E") is not self-evident
--   attempted via intermediate: reachable("Y", "E") cannot be established
--     (no outgoing edges from "Y")
```

**Assessment:** The two `reachable` claims with the same name but different bodies demonstrate the "or" of multiple clauses elegantly — the syntax allows a claim to have multiple derivation rules simply by declaring them more than once. The evidence output showing an alternative derivation path illustrates first-class evidence well: multiple proofs exist for the same fact. The comment about clause ordering being irrelevant is important here — a reader might worry the runtime picks the "wrong" path, but the semantics guarantee any valid derivation is acceptable.

---

## 5. FizzBuzz

```evident
-- FizzBuzz
-- Establish the correct FizzBuzz label for a given integer n.

claim divisible_by_3(n) is established when
  n mod 3 = 0

claim divisible_by_5(n) is established when
  n mod 5 = 0

claim fizzbuzz(n, result: "FizzBuzz") is established when
  divisible_by_3(n) is established
  and divisible_by_5(n) is established

claim fizzbuzz(n, result: "Fizz") is established when
  divisible_by_3(n) is established
  and divisible_by_5(n) is not established

claim fizzbuzz(n, result: "Buzz") is established when
  divisible_by_5(n) is established
  and divisible_by_3(n) is not established

claim fizzbuzz(n, result: n) is established when
  divisible_by_3(n) is not established
  and divisible_by_5(n) is not established

-- Query: establish fizzbuzz for n = 15

establish fizzbuzz(n: 15, result: label)
  and show evidence for label

-- Evidence reads:
-- label = "FizzBuzz"
-- because fizzbuzz(15, "FizzBuzz") is established
--   because divisible_by_3(15) is established
--     because 15 mod 3 = 0
--   and divisible_by_5(15) is established
--     because 15 mod 5 = 0

-- Query: establish fizzbuzz for all n from 1 to 20

for all n where n >= 1 and n <= 20
  establish fizzbuzz(n, result: label)
  and show label
```

**Assessment:** The use of `is not established` for negation reads naturally and makes the mutual-exclusion logic clear to any reader — no special syntax is needed. The `result: "FizzBuzz"` inline binding in the claim head is expressive and avoids a separate equality step in the body. The `for all n where` query form at the end hints at a batch-query capability that feels natural in English but would need careful semantics — specifically, whether it produces a relation or triggers output as a side effect.

---

## Overall Assessment

The English DSL succeeds at its primary goal: a non-programmer can read any of these five programs and reconstruct the intent without knowing what "unification" or "backtracking" means. The `claim ... is established when` frame is versatile enough to handle base cases, recursive cases, conjunction, and negation without introducing new keywords for each. The `is self-evident` form for axioms is particularly strong — it signals "this is a ground truth, take it as given" without any ceremony.

The main friction points are: (1) arithmetic and built-in predicates require an implicit standard library that the syntax alone cannot define, (2) intermediate bindings like `n_minus_one = n - 1` interrupt the English flow, and (3) the `is not established` negation is readable but hides the distinction between "provably false" and "unprovable" — a subtlety that matters in a fixpoint semantics. The evidence output format, shown as comments, demonstrates that derivation trees can be surfaced in a form that reads like a natural explanation, which is a genuine strength of the approach.
