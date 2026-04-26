# Syntax: Python-Inspired

This syntax dresses Evident's logic programming model in Python's clothing: significant indentation replaces clause delimiters, decorators mark claim-providers, and `require()` reads like a familiar function call. The goal is to let a Python programmer feel immediately at home — decorators, type hints, dataclasses, and generator-style `yield` are all borrowed directly. The tradeoff is that Python's sequential, imperative associations fight against Evident's unordered, declarative semantics. The decorator stack communicates multiplicity (many functions with the same name are alternative clauses), but Python programmers will instinctively expect the last definition to win. Leaning into that friction honestly is part of what this exploration surfaces.

---

## 1. Factorial

```evident
from evident import evident, infer, evidence
from dataclasses import dataclass
from typing import NamedTuple

@dataclass
class FactorialEvidence:
    n: int
    result: int
    sub: "FactorialEvidence | None"

# Base case: 0! = 1
# @evident.base marks a self-evident (axiom) clause — no sub-claims needed
@evident.base
def factorial(n: int) -> FactorialEvidence:
    require(n == 0)
    return evidence(FactorialEvidence(n=0, result=1, sub=None))

# Recursive case: N! = N * (N-1)!
# Multiple functions with the same name and @evident = alternative clauses
# The runtime may try either clause; ordering is irrelevant
@evident
def factorial(n: int) -> FactorialEvidence:
    require(n > 0)
    sub = require(factorial(n - 1))       # sub is a FactorialEvidence term
    result = n * sub.result
    return evidence(FactorialEvidence(n=n, result=result, sub=sub))

# Ask the runtime for evidence that factorial(5) holds
fact5 = infer(factorial(5))
print(fact5.result)   # 120
print(fact5)          # full derivation tree, inspectable as a dataclass
```

The base case and recursive case read almost like ordinary Python overloads, which is the best outcome for familiarity. The awkward moment is `sub = require(factorial(n - 1))`: `require` normally asserts a condition and returns nothing in test frameworks, so doubling it as the mechanism that *retrieves* evidence is a semantic overload that takes adjustment. The dataclass evidence terms fit Python's ecosystem naturally — they can be serialized to JSON with `dataclasses.asdict()` without any special tooling.

---

## 2. Sorted List

```evident
from evident import evident, infer, evidence, assert_fact
from dataclasses import dataclass
from typing import Generic, TypeVar

T = TypeVar("T")

@dataclass
class SortedEvidence:
    case: str                        # "empty" | "singleton" | "cons"
    lst: list
    head_le_next: bool | None        # only meaningful for "cons"
    sub: "SortedEvidence | None"     # recursive sub-evidence

# Clause 1: empty list is sorted
@evident.base
def sorted_list(lst: list) -> SortedEvidence:
    require(lst == [])
    return evidence(SortedEvidence(case="empty", lst=lst,
                                   head_le_next=None, sub=None))

# Clause 2: singleton list is sorted
@evident.base
def sorted_list(lst: list) -> SortedEvidence:
    require(len(lst) == 1)
    return evidence(SortedEvidence(case="singleton", lst=lst,
                                   head_le_next=None, sub=None))

# Clause 3: [head, next, *rest] is sorted when head <= next
#           and [next, *rest] is sorted
@evident
def sorted_list(lst: list) -> SortedEvidence:
    require(len(lst) >= 2)
    head, next_, *rest = lst
    require(head <= next_)
    sub = require(sorted_list([next_] + rest))
    return evidence(SortedEvidence(case="cons", lst=lst,
                                   head_le_next=True, sub=sub))

# Usage
ev = infer(sorted_list([1, 3, 5, 7]))
print(ev.case)       # "cons"
print(ev.sub.case)   # "cons"
# Walk the full derivation tree
def depth(e: SortedEvidence) -> int:
    return 0 if e.sub is None else 1 + depth(e.sub)
print(depth(ev))     # 3
```

Three clauses with three `@evident` decorators on the same name reads cleanly as "three ways to establish `sorted_list`." The unpacking line (`head, next_, *rest = lst`) feels idiomatic Python inside what is nominally a logic clause, which is a pleasant surprise. The repetition of the `@dataclass` evidence type across three clauses that all return the same type starts to feel verbose for a more complex domain; a richer evidence DSL or `@evident.evidence_type` decorator might help.

---

## 3. HTTP Request Validation

```evident
from evident import evident, infer, evidence, assert_fact
from dataclasses import dataclass, field
from typing import Any
import json

@dataclass
class AuthEvidence:
    token: str
    user_id: str
    scopes: list[str]

@dataclass
class BodyEvidence:
    raw: bytes
    parsed: dict[str, Any]
    content_type: str

@dataclass
class RequestEvidence:
    method: str
    path: str
    auth: AuthEvidence
    body: BodyEvidence
    warnings: list[str] = field(default_factory=list)

# Ground facts: known valid tokens (asserted at startup or from a DB)
assert_fact(valid_token("tok-abc123", user_id="u1", scopes=["read", "write"]))
assert_fact(valid_token("tok-xyz789", user_id="u2", scopes=["read"]))

# Sub-claim: the request carries a valid auth token
@evident
def authenticated(headers: dict) -> AuthEvidence:
    require("Authorization" in headers)
    raw = headers["Authorization"]
    require(raw.startswith("Bearer "))
    token = raw.removeprefix("Bearer ").strip()
    tok_ev = require(valid_token(token))          # uses the asserted facts above
    return evidence(AuthEvidence(token=token,
                                 user_id=tok_ev.user_id,
                                 scopes=tok_ev.scopes))

# Sub-claim: the request body is valid JSON
@evident
def valid_body(headers: dict, raw_body: bytes) -> BodyEvidence:
    ct = headers.get("Content-Type", "")
    require("application/json" in ct)
    try:
        parsed = json.loads(raw_body)
    except json.JSONDecodeError as exc:
        require(False, reason=f"JSON parse error: {exc}")
    return evidence(BodyEvidence(raw=raw_body, parsed=parsed,
                                 content_type=ct))

# Top-level claim: the full request is valid
@evident
def valid_request(method: str, path: str,
                  headers: dict, raw_body: bytes) -> RequestEvidence:
    auth_ev  = require(authenticated(headers))
    body_ev  = require(valid_body(headers, raw_body))
    warnings = []
    if "write" not in auth_ev.scopes and method in ("POST", "PUT", "PATCH"):
        warnings.append("Token lacks write scope for mutating method")
    return evidence(RequestEvidence(method=method, path=path,
                                    auth=auth_ev, body=body_ev,
                                    warnings=warnings))

# Usage: try to validate an incoming request
import sys

req_ev = infer(valid_request(
    method="POST",
    path="/api/items",
    headers={"Authorization": "Bearer tok-abc123",
             "Content-Type": "application/json"},
    raw_body=b'{"name": "widget"}',
))

if req_ev is None:
    # infer() returns None when no derivation exists
    print("Request rejected — no valid derivation found", file=sys.stderr)
else:
    print(f"Accepted as user {req_ev.auth.user_id}")
    if req_ev.warnings:
        print("Warnings:", req_ev.warnings)
    # Sub-evidence is directly accessible for logging or audit trails
    audit = {
        "user_id":      req_ev.auth.user_id,
        "scopes":       req_ev.auth.scopes,
        "parsed_body":  req_ev.body.parsed,
        "warnings":     req_ev.warnings,
    }
    print(json.dumps(audit, indent=2))
```

The layered claim structure maps elegantly onto Python's function composition intuitions: `valid_request` calls `authenticated` and `valid_body` the way a normal Python function would call helpers, except the runtime is responsible for finding derivations rather than the caller controlling execution order. Accessing sub-evidence (`req_ev.auth.user_id`, `req_ev.body.parsed`) feels completely natural — it is just attribute access on a dataclass. The one awkward seam is `require(False, reason=...)` as the idiom for claiming a dead end; it looks like an assertion failure rather than a logical refutation, and a dedicated `refute(reason=...)` call would be cleaner.

---

## 4. Graph Reachability

```evident
from evident import evident, infer, evidence, assert_fact
from dataclasses import dataclass

@dataclass
class EdgeEvidence:
    src: str
    dst: str

@dataclass
class ReachableEvidence:
    src: str
    dst: str
    path: list[str]
    derivation: list["EdgeEvidence | ReachableEvidence"]

# Ground edges: assert the adjacency relation as base facts
# These could be loaded from a database, config file, or network call
assert_fact(edge("A", "B"))
assert_fact(edge("B", "C"))
assert_fact(edge("C", "D"))
assert_fact(edge("A", "D"))
assert_fact(edge("D", "E"))

# Clause 1: direct edge — one hop reachability
@evident
def reachable(src: str, dst: str) -> ReachableEvidence:
    e = require(edge(src, dst))
    return evidence(ReachableEvidence(
        src=src, dst=dst,
        path=[src, dst],
        derivation=[e],
    ))

# Clause 2: transitive — src reaches mid, mid reaches dst
# The runtime explores alternatives; no explicit visited-set needed here
# (cycle detection is a runtime concern, not a clause concern)
@evident
def reachable(src: str, dst: str) -> ReachableEvidence:
    mid_ev  = require(reachable(src, ...))        # ... = any intermediate node
    tail_ev = require(reachable(mid_ev.dst, dst))
    return evidence(ReachableEvidence(
        src=src, dst=dst,
        path=mid_ev.path + tail_ev.path[1:],      # stitch paths, avoid dup mid
        derivation=[mid_ev, tail_ev],
    ))

# Usage
ev = infer(reachable("A", "E"))
if ev:
    print("Path:", " -> ".join(ev.path))          # e.g. A -> B -> C -> D -> E
    print("Hops:", len(ev.path) - 1)

# Collect ALL derivations (all paths), not just the first
all_paths = infer.all(reachable("A", "E"))
for p in all_paths:
    print(" -> ".join(p.path))

# Sub-evidence as a structured audit log
def derivation_summary(ev: ReachableEvidence, indent=0) -> str:
    prefix = "  " * indent
    lines = [f"{prefix}{ev.src} -> {ev.dst}  (path: {ev.path})"]
    for step in ev.derivation:
        if isinstance(step, ReachableEvidence):
            lines.append(derivation_summary(step, indent + 1))
        else:
            lines.append(f"{prefix}  edge({step.src}, {step.dst})")
    return "\n".join(lines)

print(derivation_summary(ev))
```

`assert_fact` for graph edges reads like a natural data-loading step and would compose well with a database query that bulk-asserts edges at startup. The wildcard `...` in `require(reachable(src, ...))` is a borrowed Python idiom (Ellipsis) repurposed as an existential variable, which is visually lightweight but semantically surprising — a newcomer might read it as "pass nothing here." The `infer.all(...)` extension for collecting all derivations is a small but important affordance that fits naturally as a method on the `infer` object.

---

## 5. FizzBuzz

```evident
from evident import evident, infer, evidence
from dataclasses import dataclass

@dataclass
class FizzBuzzEvidence:
    n: int
    label: str
    divisible_by_3: bool
    divisible_by_5: bool

# Four clauses, each guarded by a condition
# @evident.when(cond) is syntactic sugar: the clause is only attempted
# when cond evaluates to True; it is not a filter applied after the fact

@evident.when(lambda n: n % 3 == 0 and n % 15 == 0)
def fizzbuzz(n: int) -> FizzBuzzEvidence:
    return evidence(FizzBuzzEvidence(n=n, label="FizzBuzz",
                                     divisible_by_3=True,
                                     divisible_by_5=True))

@evident.when(lambda n: n % 3 == 0 and n % 15 != 0)
def fizzbuzz(n: int) -> FizzBuzzEvidence:
    return evidence(FizzBuzzEvidence(n=n, label="Fizz",
                                     divisible_by_3=True,
                                     divisible_by_5=False))

@evident.when(lambda n: n % 5 == 0 and n % 15 != 0)
def fizzbuzz(n: int) -> FizzBuzzEvidence:
    return evidence(FizzBuzzEvidence(n=n, label="Buzz",
                                     divisible_by_3=False,
                                     divisible_by_5=True))

@evident.when(lambda n: n % 3 != 0 and n % 5 != 0)
def fizzbuzz(n: int) -> FizzBuzzEvidence:
    return evidence(FizzBuzzEvidence(n=n, label=str(n),
                                     divisible_by_3=False,
                                     divisible_by_5=False))

# Usage: run FizzBuzz 1–20, collecting evidence for each
results = [infer(fizzbuzz(n)) for n in range(1, 21)]
for ev in results:
    print(ev.label)

# Because evidence is first-class, you can filter or group by derivation
fizz_only = [ev for ev in results if ev.divisible_by_3 and not ev.divisible_by_5]
print(f"Pure Fizz numbers: {[ev.n for ev in fizz_only]}")

# Evidence can be serialized trivially
import json, dataclasses
evidence_log = [dataclasses.asdict(ev) for ev in results]
with open("fizzbuzz_evidence.json", "w") as f:
    json.dump(evidence_log, f, indent=2)
```

The `@evident.when(lambda n: ...)` guard decorator stacks naturally above the function definition and reads almost like a Python `@pytest.mark.skipif` — the convention is familiar. The redundant divisibility checks across clauses (e.g., checking `n % 15 != 0` in both the "Fizz" and "Buzz" guards) reveal a weakness: guards are attached to individual clauses rather than being composed, so the programmer must manually partition the guard space. A future `@evident.when` might support pattern-match-style exclusion automatically. Serializing evidence to JSON at the end showcases the ecosystem integration story compellingly with almost no extra code.

---

## Overall Assessment

The Python-inspired syntax achieves its primary goal: a Python programmer can read these programs and understand the structure without learning a new notation from scratch. Decorators for clause multiplicity, `require()` for sub-claims, and dataclass evidence terms all land in familiar territory.

The deepest tension is the redefinition of function names. Python programmers expect the last definition of a function to shadow earlier ones; here, all definitions coexist as alternatives. This is the single largest conceptual hurdle, and no amount of syntactic sugar fully dissolves it — a runtime warning or IDE plugin that highlights "these are alternative clauses, not overrides" would be a necessary companion to the syntax.

`require()` is doing heavy lifting: it both asserts a condition (`require(n > 0)`) and retrieves evidence (`sub = require(factorial(n - 1))`). These two uses are meaningfully different and arguably deserve different names — perhaps `guard(n > 0)` for pure conditions and `require(claim)` only for sub-claim retrieval.

The ecosystem integration story — JSON serialization, logging, audit trails — is the syntax's strongest selling point. Evidence-as-dataclass means the entire Python data ecosystem (Pydantic, dataclasses, JSON, SQLAlchemy) works without adapters.
