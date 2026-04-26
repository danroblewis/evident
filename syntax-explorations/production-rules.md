# Syntax: Production Rules

This syntax frames Evident as a **business rule engine**: every rule has a name, a `WHEN` section listing the conditions that must hold, and a `THEN` section declaring what becomes evident as a result. Facts are declared explicitly with `FACT` or `ASSERT`. Variables are bound with the `?name` prefix, borrowed from CLIPS. Keywords are ALL-CAPS to visually separate the structure of the rule from its content — a deliberate choice for domain-expert readability. The model is a fixpoint: rules fire repeatedly until no new facts can be derived. `QUERY` asks the engine what is evident; `EXPLAIN` surfaces the derivation tree. Optional `PRIORITY` hints guide conflict resolution without changing semantics.

---

## 1. Factorial

```evident
-- Factorial via production rules.
-- Each rule fires once per binding of ?n, asserting factorial(?n, ?result).
-- The engine keeps applying rules until the fixpoint is reached.

RULE base-case
WHEN
    -- No conditions: this rule fires once unconditionally.
THEN
    ASSERT factorial(0, 1)

RULE recursive-case
PRIORITY 10
WHEN
    factorial(?m, ?k)         -- a result for m is already known
    ?n = ?m + 1               -- n is the next number up
THEN
    ASSERT factorial(?n, ?n * ?k)

-- Ask for factorial of 5:
QUERY factorial(5, ?result)

-- Expected answer: factorial(5, 120)

-- Ask with derivation trace:
EXPLAIN factorial(5, ?result)

-- EXPLAIN output:
-- factorial(5, 120)
--   via recursive-case
--     factorial(4, 24)
--       via recursive-case
--         factorial(3, 6)
--           via recursive-case
--             factorial(2, 2)
--               via recursive-case
--                 factorial(1, 1)
--                   via recursive-case
--                     factorial(0, 1)
--                       via base-case
```

The chain-firing structure maps naturally onto Evident's fixpoint semantics: each rule application asserts one new fact, which immediately enables the next firing. What feels awkward is that the `recursive-case` builds upward from 0 rather than downward from N — the rule system cannot directly express "decompose 5 into 4 + 1"; it must instead enumerate upward from the base case. This inverts the intuitive top-down reading of recursion and requires the base case to fire first as a seed, which feels more like bottom-up Datalog than recursive decomposition.

---

## 2. Sorted List

```evident
-- A sorted list is represented as a chain of facts.
-- sorted-step(A, B) means A <= B and both are adjacent in the list.
-- sorted-list(?id) is evident when all adjacent pairs in list ?id are sorted.

-- Declare the list as a series of adjacent-pair facts:
FACT list-pair(list-id: "my-list", left: 1,  right: 2)
FACT list-pair(list-id: "my-list", left: 2,  right: 5)
FACT list-pair(list-id: "my-list", left: 5,  right: 9)
FACT list-pair(list-id: "my-list", left: 9,  right: 12)

-- A pair is in-order if left <= right:
RULE pair-in-order
WHEN
    list-pair(list-id: ?id, left: ?a, right: ?b)
    ?a <= ?b
THEN
    ASSERT pair-sorted(list-id: ?id, left: ?a, right: ?b)

-- A list is sorted if every pair in it is sorted.
-- We check by asserting unsorted-pair when a pair is NOT sorted:
RULE pair-out-of-order
WHEN
    list-pair(list-id: ?id, left: ?a, right: ?b)
    ?a > ?b
THEN
    ASSERT unsorted-pair(list-id: ?id, left: ?a, right: ?b)

-- The list is sorted if no unsorted pairs exist:
RULE list-is-sorted
WHEN
    NOT unsorted-pair(list-id: ?id)
    -- At least one pair exists (guards against empty/missing lists):
    list-pair(list-id: ?id, left: ?any-a, right: ?any-b)
THEN
    ASSERT sorted-list(?id)

-- The list is unsorted if any unsorted pair exists:
RULE list-is-unsorted
WHEN
    unsorted-pair(list-id: ?id, left: ?a, right: ?b)
THEN
    ASSERT unsorted-list(?id)

-- Query:
QUERY sorted-list("my-list")

-- Counter-example: declare a list that is not sorted
FACT list-pair(list-id: "bad-list", left: 3, right: 1)
FACT list-pair(list-id: "bad-list", left: 1, right: 9)

QUERY sorted-list("bad-list")     -- not evident
QUERY unsorted-list("bad-list")   -- evident

EXPLAIN unsorted-list("bad-list")
-- unsorted-list("bad-list")
--   via list-is-unsorted
--     unsorted-pair(list-id: "bad-list", left: 3, right: 1)
--       via pair-out-of-order
--         list-pair(list-id: "bad-list", left: 3, right: 1)  [FACT]
--         3 > 1  [built-in]
```

The slot-based representation of list structure maps cleanly onto the rule engine's relational model — each `list-pair` fact is essentially a row in a table, and rules operate like SQL over that table. The `NOT` condition for detecting a sorted list reads naturally for someone coming from a business rule background. The awkwardness is that list structure must be pre-exploded into pair facts; there is no compact syntactic form for `[1, 2, 5, 9, 12]`. A domain expert working with actual lists would need a preprocessing step to shred the list before the rules can fire.

---

## 3. HTTP Request Validation

```evident
-- HTTP request validation.
-- Each aspect of validity is a separate rule, establishing a partial-validity fact.
-- The final rule fires only when all checks have passed.

-- Declare an incoming request as a fact:
FACT request(
    id:           "req-001",
    method:       "POST",
    path:         "/api/orders",
    auth-header:  "Bearer eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ1c2VyLTQyIn0.abc123",
    content-type: "application/json",
    body-size:    412
)

-- Known valid API keys (in practice, these come from a key store):
FACT valid-token("eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ1c2VyLTQyIn0.abc123", user-id: "user-42")

-- Rule 1: Method must be an allowed HTTP verb.
RULE check-method
WHEN
    request(id: ?req-id, method: ?method)
    ?method in ["GET", "POST", "PUT", "DELETE", "PATCH"]
THEN
    ASSERT method-valid(req-id: ?req-id, method: ?method)

-- Rule 2: Auth header must be a Bearer token and the token must be in the key store.
RULE check-auth
WHEN
    request(id: ?req-id, auth-header: ?header)
    ?scheme ?token = SPLIT(?header, " ", 2)
    ?scheme = "Bearer"
    valid-token(?token, user-id: ?user-id)
THEN
    ASSERT auth-valid(req-id: ?req-id, user-id: ?user-id, token: ?token)

-- Rule 3: Content-Type must be application/json for mutation methods.
RULE check-content-type-mutation
WHEN
    request(id: ?req-id, method: ?method, content-type: ?ct)
    ?method in ["POST", "PUT", "PATCH"]
    ?ct = "application/json"
THEN
    ASSERT content-type-valid(req-id: ?req-id)

-- Rule 3b: GET and DELETE do not require a content-type.
RULE check-content-type-readonly
WHEN
    request(id: ?req-id, method: ?method)
    ?method in ["GET", "DELETE"]
THEN
    ASSERT content-type-valid(req-id: ?req-id)

-- Rule 4: Body must not be excessively large (>= 10MB is rejected).
RULE check-body-size
WHEN
    request(id: ?req-id, body-size: ?size)
    ?size < 10485760
THEN
    ASSERT body-size-valid(req-id: ?req-id)

-- Final rule: all checks must pass to declare the request valid.
RULE request-fully-valid
WHEN
    method-valid(req-id: ?req-id)
    auth-valid(req-id: ?req-id, user-id: ?user-id)
    content-type-valid(req-id: ?req-id)
    body-size-valid(req-id: ?req-id)
THEN
    ASSERT request-valid(req-id: ?req-id, authorized-user: ?user-id)

-- Query whether the request passed:
QUERY request-valid(req-id: "req-001", authorized-user: ?user)

-- Explain the full derivation:
EXPLAIN request-valid(req-id: "req-001", authorized-user: ?user)

-- EXPLAIN output:
-- request-valid(req-id: "req-001", authorized-user: "user-42")
--   via request-fully-valid
--     method-valid(req-id: "req-001", method: "POST")
--       via check-method
--         request(id: "req-001", method: "POST", ...)  [FACT]
--         "POST" in ["GET", "POST", ...]               [built-in]
--     auth-valid(req-id: "req-001", user-id: "user-42", token: "eyJ...")
--       via check-auth
--         request(id: "req-001", auth-header: "Bearer eyJ...")  [FACT]
--         SPLIT("Bearer eyJ...", " ", 2) = ["Bearer", "eyJ..."] [built-in]
--         "Bearer" = "Bearer"                                    [built-in]
--         valid-token("eyJ...", user-id: "user-42")             [FACT]
--     content-type-valid(req-id: "req-001")
--       via check-content-type-mutation
--         request(id: "req-001", method: "POST", content-type: "application/json")  [FACT]
--         "POST" in ["POST", "PUT", "PATCH"]                                        [built-in]
--         "application/json" = "application/json"                                  [built-in]
--     body-size-valid(req-id: "req-001")
--       via check-body-size
--         request(id: "req-001", body-size: 412)  [FACT]
--         412 < 10485760                          [built-in]

-- Failure scenario: invalid token
FACT request(
    id:           "req-002",
    method:       "POST",
    path:         "/api/orders",
    auth-header:  "Bearer bad-token-xyz",
    content-type: "application/json",
    body-size:    100
)

QUERY request-valid(req-id: "req-002", authorized-user: ?user)
-- Not evident.

EXPLAIN NOT request-valid(req-id: "req-002", authorized-user: ?user)
-- NOT request-valid(req-id: "req-002", ...)
--   request-fully-valid did not fire: auth-valid(req-id: "req-002") was never asserted
--     check-auth could not fire: valid-token("bad-token-xyz", ...) not in fact base
```

This is the natural habitat for production-rule syntax — validating a structured data object against multiple independent criteria reads like a compliance checklist. Named rules (`check-method`, `check-auth`) give the domain expert a vocabulary that maps directly to their policy documents. The `EXPLAIN` output is especially valuable here: when a request fails, it shows exactly which criterion was unmet and why, providing actionable diagnostics. The awkward moment is the `SPLIT` call inside `check-auth` — procedural string manipulation sits uneasily in a declarative rule condition, and the two-value destructuring `?scheme ?token = SPLIT(...)` is not consistent with the slot-matching style used elsewhere.

---

## 4. Graph Reachability

```evident
-- Graph reachability via production rules.
-- Direct edges are asserted as facts.
-- Two rules derive the transitive closure.

-- Declare the graph edges:
FACT edge(from: "london",  to: "paris")
FACT edge(from: "paris",   to: "berlin")
FACT edge(from: "berlin",  to: "warsaw")
FACT edge(from: "warsaw",  to: "kyiv")
FACT edge(from: "london",  to: "amsterdam")
FACT edge(from: "amsterdam", to: "brussels")
FACT edge(from: "brussels", to: "paris")

-- Rule 1: A direct edge implies reachability.
RULE direct-reachability
WHEN
    edge(from: ?a, to: ?b)
THEN
    ASSERT reachable(from: ?a, to: ?b)

-- Rule 2: Transitivity. If A reaches B and B reaches C, then A reaches C.
-- The engine applies this repeatedly until no new reachable facts appear.
RULE transitive-reachability
WHEN
    reachable(from: ?a, to: ?b)
    reachable(from: ?b, to: ?c)
    ?a != ?c           -- prevent trivial self-loops
THEN
    ASSERT reachable(from: ?a, to: ?c)

-- Queries:
QUERY reachable(from: "london", to: "warsaw")      -- evident (london→paris→berlin→warsaw)
QUERY reachable(from: "london", to: "kyiv")        -- evident (london→paris→berlin→warsaw→kyiv)
QUERY reachable(from: "warsaw", to: "london")      -- NOT evident (no back edges)
QUERY reachable(from: "london", to: ?destination)  -- enumerate all reachable cities

EXPLAIN reachable(from: "london", to: "warsaw")
-- reachable(from: "london", to: "warsaw")
--   via transitive-reachability
--     reachable(from: "london", to: "berlin")
--       via transitive-reachability
--         reachable(from: "london", to: "paris")
--           via direct-reachability
--             edge(from: "london", to: "paris")  [FACT]
--         reachable(from: "paris", to: "berlin")
--           via direct-reachability
--             edge(from: "paris", to: "berlin")  [FACT]
--     reachable(from: "berlin", to: "warsaw")
--       via direct-reachability
--         edge(from: "berlin", to: "warsaw")  [FACT]

-- Find all cities reachable from london:
QUERY reachable(from: "london", to: ?city)
-- Results:
--   reachable(from: "london", to: "paris")
--   reachable(from: "london", to: "amsterdam")
--   reachable(from: "london", to: "brussels")
--   reachable(from: "london", to: "berlin")
--   reachable(from: "london", to: "warsaw")
--   reachable(from: "london", to: "kyiv")
```

Graph reachability is one of the canonical success stories for production rule systems — this is essentially Datalog, and the two-rule structure mirrors the textbook transitive closure program almost exactly. The named-slot style (`from:`, `to:`) makes the rules more readable than positional arguments; it is immediately clear which variable represents the source and which the destination. The `?a != ?c` guard for self-loops is necessary but feels bolted on — a more principled acyclicity mechanism would be cleaner. The fixpoint semantics shine here: the engine naturally saturates the reachability relation without the programmer having to manage a worklist.

---

## 5. FizzBuzz

```evident
-- FizzBuzz via production rules with PRIORITY-based conflict resolution.
-- Four rules can fire for each number; PRIORITY determines which label wins
-- when multiple rules match.
--
-- Priority convention (higher fires first / takes precedence when conflicts arise):
--   PRIORITY 30 = fizzbuzz  (most specific: divisible by both)
--   PRIORITY 20 = fizz      (divisible by 3)
--   PRIORITY 20 = buzz      (divisible by 5)
--   PRIORITY 10 = plain     (fallback: none of the above)
--
-- Because THEN asserts a unique label per number, once the highest-priority
-- rule fires for a given ?n it satisfies the label claim; lower-priority rules
-- do not fire redundantly (the fact is already asserted).

-- Seed: declare which numbers we want to label (1 through 20):
FACT number(1)  FACT number(2)  FACT number(3)  FACT number(4)  FACT number(5)
FACT number(6)  FACT number(7)  FACT number(8)  FACT number(9)  FACT number(10)
FACT number(11) FACT number(12) FACT number(13) FACT number(14) FACT number(15)
FACT number(16) FACT number(17) FACT number(18) FACT number(19) FACT number(20)

-- Rule 1: FizzBuzz — divisible by both 3 and 5. Most specific; highest priority.
RULE fizzbuzz
PRIORITY 30
WHEN
    number(?n)
    ?n MOD 3 = 0
    ?n MOD 5 = 0
THEN
    ASSERT label(?n, "FizzBuzz")

-- Rule 2: Fizz — divisible by 3 only.
RULE fizz
PRIORITY 20
WHEN
    number(?n)
    ?n MOD 3 = 0
    NOT label(?n, ?any)       -- only if no label has been assigned yet
THEN
    ASSERT label(?n, "Fizz")

-- Rule 3: Buzz — divisible by 5 only.
RULE buzz
PRIORITY 20
WHEN
    number(?n)
    ?n MOD 5 = 0
    NOT label(?n, ?any)       -- only if no label has been assigned yet
THEN
    ASSERT label(?n, "Buzz")

-- Rule 4: Plain number — none of the above.
RULE plain
PRIORITY 10
WHEN
    number(?n)
    NOT label(?n, ?any)       -- fires only if no other rule assigned a label
THEN
    ASSERT label(?n, TO-STRING(?n))

-- Query all results, ordered by number:
QUERY label(?n, ?text) ORDER BY ?n

-- Expected output:
-- label(1,  "1")        label(2,  "2")        label(3,  "Fizz")
-- label(4,  "4")        label(5,  "Buzz")      label(6,  "Fizz")
-- label(7,  "7")        label(8,  "8")        label(9,  "Fizz")
-- label(10, "Buzz")     label(11, "11")       label(12, "Fizz")
-- label(13, "13")       label(14, "14")       label(15, "FizzBuzz")
-- label(16, "16")       label(17, "17")       label(18, "Fizz")
-- label(19, "19")       label(20, "Buzz")

EXPLAIN label(15, ?text)
-- label(15, "FizzBuzz")
--   via fizzbuzz [PRIORITY 30]
--     number(15)       [FACT]
--     15 MOD 3 = 0     [built-in]
--     15 MOD 5 = 0     [built-in]

-- Conflict trace for n=15 (showing why fizz and buzz did not fire):
-- fizz  [PRIORITY 20]: did not fire — label(15, "FizzBuzz") already asserted by higher-priority rule
-- buzz  [PRIORITY 20]: did not fire — label(15, "FizzBuzz") already asserted by higher-priority rule
-- plain [PRIORITY 10]: did not fire — label(15, "FizzBuzz") already asserted by higher-priority rule
```

The PRIORITY mechanism handles the FizzBuzz overlap cleanly: the most specific rule fires first, and the `NOT label(?n, ?any)` guards on lower-priority rules ensure no number gets labeled twice. This is explicit and auditable — a domain expert can read the priority ladder and understand the precedence without knowing anything about execution order. The awkward part is the `NOT label(?n, ?any)` pattern: it requires stratified negation-as-failure, which means the engine must know that `fizzbuzz` has already reached its fixpoint before `fizz` can safely evaluate the `NOT`. This creates a hidden temporal dependency between rules that the PRIORITY annotation does not fully capture.

---

## Overall Assessment

The production-rule syntax is the clearest expression of Evident's fixpoint semantics. The visual structure — `RULE`, `WHEN`, `THEN` — maps the mental model of "conditions → conclusions" onto the page without ambiguity, and `PRIORITY` gives domain experts a lever for conflict resolution that does not require understanding the underlying evaluation order. `EXPLAIN` is the syntax's greatest strength: because every derived fact traces back through a named rule to named facts, the output is human-readable prose, not an opaque stack trace.

The principal tension is between the **relational worldview** the syntax assumes and the **structural data** programs actually handle. Lists must be shredded into pair facts; strings must be manipulated with `SPLIT` and `TO-STRING` calls that look procedural sitting inside declarative rule conditions; recursive functions must be expressed as upward-propagating chains rather than top-down decompositions. These are not unsolvable problems, but they reveal that production-rule systems are most comfortable when data arrives pre-normalized into ground facts. Any program that must first transform its input before reasoning about it will have an awkward seam between the transformation and the rules.

For the target audience — a compliance analyst or domain architect who will maintain these rules long after the initial author is gone — this syntax earns its keep. Rules are individually named, independently testable, and self-documenting. The cost is verbosity and the loss of the compact recursive style that makes programs like factorial and sorted-list feel natural in functional or Prolog-style syntax.
