# Function-izer bench — 242× on Pair

The function-izer pipeline lands end-to-end. The proof: a claim
that is function-shaped given its inputs compiles to a substitution
chain, evaluates natively (pure Rust AST tree-walk, no Z3), and
produces identical results to the solver.

## The claim

```evident
claim Pair
    a ∈ Int
    b ∈ Int
    sum ∈ Int
    prod ∈ Int
    diff ∈ Int
    neg_a ∈ Int
    sum   = a + b
    prod  = a * b
    diff  = a - b
    neg_a = 0 - a
```

With `a=5, b=3` pinned, every output is uniquely determined:
sum=8, prod=15, diff=2, neg_a=-5.

## Pipeline (4 stages, each implemented and tested)

```
1. classify_components(claim, given)
   ↓ identifies which components are function-shaped
   ↓ via the 2-copy uniqueness check (1 Z3 call per component)
2. extract_chain(schema, component)
   ↓ walks the schema body, finds `var = expr` equalities
   ↓ topo-sorts substitutions by dependency
   ↓ returns a SubstitutionChain or None
3. evaluate_chain(chain, given)
   ↓ pure Rust interpreter walks each step's Expr
   ↓ produces the full binding map
4. (eventually) cache by (schema, given-keys); hook into rt.query
```

## Bench

`runtime/examples/bench_functionize.rs`, 10,000 iterations each:

| Path | μs/call | Total |
|---|---|---|
| **Z3 query** | 99.27 | 992.7 ms |
| **Native chain** | 0.41 | 4.1 ms |

**242× speedup** on this workload. The native path takes ~0.4
microseconds — essentially just the cost of HashMap lookups and a
few arithmetic ops.

Both paths produce identical results, verified per-binding.

## Why this works

For function-shaped claims, Z3 is doing a lot of avoidable work:

- Building a fresh Solver per query
- Parsing the schema into Z3 sorts / constraints (re-translation each call)
- Running tactic preprocessing (even after our solve-eqs default)
- Running the solver core
- Extracting the model back into Value form

For Pair specifically, the actual computation is `let sum = a + b;
let prod = a * b; let diff = a - b; let neg_a = -a;` — four
trivial arithmetic ops, totalling nanoseconds. Z3's overhead
dominates by 240×.

## Scope of v1 function-izer

What works today:
- Substitution extraction from direct `var = expr` equalities in
  the schema body (top-level `BodyItem::Constraint`s).
- Topo-sort by dependency, cycle detection.
- Native evaluation: arithmetic, comparisons, logical ops, literals,
  identifiers, ternary, negation. Most Evident expression shapes.

What's not yet supported in extraction:
- Substitutions emerging from constraint algebra (`a + b = 10 ∧ a = 3`
  implies `b = 7` but isn't a direct `b = ...` line). Future work
  is to apply `solve-eqs` and diff the result.
- Substitutions inside Passthrough or ClaimCall bodies. The v1
  extractor only looks at top-level constraints.

What's not yet supported in evaluation:
- ∀ / ∃ quantifiers (no domain unrolling)
- Seq / Set / Record types (only primitives so far)
- Claim invocations (no recursive function calls)
- Match / Matches (no pattern dispatch)

These are mechanical extensions — each adds one or two cases to
the `mentions` walker and the `eval_expr` interpreter. Add them
as needed, on demand.

## Wired into `rt.query`

`EVIDENT_FUNCTIONIZE=1` enables the fast path inside `rt.query`. On
each call: gate the schema body (must be pure-assignment-only —
see `is_pure_assignment_body`), classify components, extract +
cache a chain, evaluate natively. Cache key is `(claim_name,
sorted_given_keys)`. Cache miss → one-time classification cost
(plus a Z3 call for the initial SAT check); cache hit → microsecond
native eval. Falls through to Z3 transparently on any miss or
extraction failure.

Bench with the wired hook (10k iterations on Pair):

```
EVIDENT_FUNCTIONIZE=0:  rt.query  = 91.91 μs/call   (pure Z3 baseline)
EVIDENT_FUNCTIONIZE=1:  rt.query  =  0.73 μs/call   (function-izer)
                                       126× speedup on rt.query path

Direct evaluate_chain (skips rt.query layer): 0.48 μs/call
```

The wired path costs ~50% more than direct `evaluate_chain` —
that's `rt.query`'s overhead (schema lookup, env-var check, cache
lookup, building the result map). Still 126× over Z3.

## Correctness gate

The gate `is_pure_assignment_body` enforces:

- Every `BodyItem::Constraint` is a `BinOp::Eq` (definition).
  Non-equality body items (filters like `n < 5`) cause the gate
  to refuse — the native evaluator wouldn't enforce them, so Z3
  must handle the claim.
- Every `BodyItem::Membership` types vars as Int / Real / Bool /
  String only. Nat, Pos, user-defined types have implicit
  type-bound constraints (n ≥ 0 for Nat, field-level constraints
  for user types) that the native path doesn't enforce.
- No `BodyItem::Passthrough` or `BodyItem::ClaimCall` — those
  reference bodies outside this schema; the v1 chain extractor
  doesn't recurse into them.

The full `./test.sh` suite (12 lints + 422 cargo tests + 119
conformance) passes with `EVIDENT_FUNCTIONIZE=1`. Claims that the
gate refuses get correctly UNSAT-handled by the Z3 fallback path
(verified with the `given_violation_unsat` test:
`schema S\n  n ∈ Nat\n  n < 5` with given n=10 correctly returns
UNSAT because `Nat` typing fails the gate, falling through to Z3).

The gate is conservative — it refuses many claims that ARE actually
function-shaped (anything with a Nat or user-type binding, anything
that uses ∀ over a Seq for iteration, anything with claim
composition). Expanding the gate to handle more shapes is direct
follow-on work; each expansion needs a soundness proof.
