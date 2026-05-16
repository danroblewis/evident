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

## Not yet wired

The bench manually invokes `classify_components` and
`extract_chain` to demonstrate the speedup. Wiring this into
`rt.query` so it transparently takes the native path when possible
is the next concrete step — small in code (a cache, a try-native-
first hook), but needs care around correctness (falling back to
Z3 when extraction fails for any reason).
