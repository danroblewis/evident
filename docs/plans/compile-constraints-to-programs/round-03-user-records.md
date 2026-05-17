# Round 3 — User-record types + Field access

**Outcome:** PARTIAL PASS. The function-izer compiles claims with
user-record-typed memberships (IVec2, Color, Mover, etc.) and dotted
field-access expressions. **114× speedup** on a record-typed Step
claim. Gate coverage went 53% → 54% (+7 claims). Mario's FSMs do
NOT yet pass — they use Passthrough, ClaimCall, ∀, and Seq which
Round 3 doesn't address.

## What was built

### `is_pure_assignment_body_full` — three-predicate gate

```rust
pub fn is_pure_assignment_body_full(
    schema: &SchemaDecl,
    is_enum: &dyn Fn(&str) -> bool,
    is_simple_record: &dyn Fn(&str) -> bool,
) -> bool
```

A "simple record" is a `type` declaration whose body has only
primitive (Int/Real/Bool/String) Memberships. Recursive records
(record of records, record of Seq) are v2 — rejecting them keeps
soundness simple.

`rt.query`'s function-izer hook builds an `is_simple_record`
predicate that consults `self.schemas` and checks the body.

### `extract_chain_full` — matching predicate

Same widening on the chain-extraction side; the rt.query hook calls
`extract_chain_full(schema, component, &is_enum, &is_simple_record)`.

### No eval-side change needed

`Expr::Field(record, "x")` doesn't appear in our AST in practice —
the parser folds `pos.x` into `Expr::Identifier("pos.x")` (a dotted
identifier). The native evaluator's existing identifier-lookup path
handles it via env. The runtime's `declare_and_assert` creates Z3
consts under the dotted name `pos.x`, `pos.y`, `pos.z` etc., and
those names flow through unchanged to the substitution chain.

## Bench

`runtime/tests/functionize_records.rs::record_typed_bench`:

```
Step (pos, vel, nxt ∈ IVec2 with per-axis arithmetic):
  Z3 query:    100.39 μs/call
  Native (fz):   0.88 μs/call
  Speedup:    114×
```

The native path is now sub-microsecond on a 4-binding record-typed
claim. Even faster than the Pair case (242× of an even smaller claim).

## Gate coverage delta

```
Round 1 baseline (primitives only):   123/460  (27%)
Round 2 + enums:                       243/460  (53%)
Round 3 + simple records:              250/460  (54%)
```

The Round 3 gain is small. Most of our codebase that uses records
ALSO uses Passthrough or ClaimCall — those still block it. The
remaining 46% of unrecognized claims fall into these categories:

- **Passthrough** (`..Level`, `..LevelConstants`) — Mario, level files.
- **ClaimCall** — subclaim invocations everywhere.
- **∀ x ∈ seq / ∀ i ∈ range** — Mario's coindexed loops, stdlib
  iteration patterns.
- **Seq / Set / EffectPair memberships** — Mario, effect dispatch.
- **Recursive records** (record fields are themselves records) —
  Mario's `Body(aabb ∈ AABB, color ∈ Color)`.
- **Non-equality constraints** — Mario's collision checks, Jumpable,
  Level1, Level2.

## Round 4 candidates

To meaningfully unlock Mario, Round 4 needs to tackle Passthrough,
recursive records, or finite-range ∀ unrolling. Estimated:

1. **Recursive simple records** (extend `is_simple_record` to allow
   record fields whose own type is also a simple record). Modest:
   ~2 days. Unlocks AABB, Body, EffectPair.
2. **Passthrough inlining** (when the gate sees a Passthrough, look
   up the referenced claim and consider its body items inline).
   Larger: 3-5 days. Has cycles + complexity concerns.
3. **∀ x ∈ {lo..hi} with pinned bounds** (unroll at chain-extract
   time into N parallel substitutions). 3-5 days. Direct unlock for
   Mario's coindexed iteration.

Round 4 will pick the next based on whichever lets at least one
Mario FSM pass the gate.
