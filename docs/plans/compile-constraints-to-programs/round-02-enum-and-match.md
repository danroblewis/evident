# Round 2 — Enum types + Match dispatch in the function-izer

**Outcome:** PASS. The function-izer now handles enum-typed Memberships
and `Match` dispatch expressions. **19× speedup** on a state-machine
claim (47.9μs Z3 → 2.5μs native). Gate coverage went from **27% to 53%**
of all claims in the codebase. All 422 cargo + 119 conformance + 12
lints pass with `EVIDENT_FUNCTIONIZE=1`.

## What was built

### 1. `Expr::Match` in `eval_expr`

Pattern dispatch handled natively:
- Scrutinee must evaluate to `Value::Enum`.
- For each arm, match by variant name; bind payload fields per
  `MatchPattern::Ctor`'s `binds` list (`Some(name)` = bind, `None` =
  wildcard skip).
- `MatchPattern::Wildcard` always matches.
- First matching arm's body is evaluated in the extended env.

### 2. Enum-variant identifier resolver

When a bare identifier like `Init` or `Done` isn't in the env, the
evaluator consults a resolver that builds `Value::Enum { enum_name,
variant, fields: vec![] }` from the runtime's `EnumRegistry`.
Limited to nullary variants in v1 — variants with payload need
`Ctor(args)` parsing (different Expr shape).

Resolver shape:
```rust
pub type IdentResolver<'a> = dyn Fn(&str) -> Option<Value> + 'a;

pub fn evaluate_chain_with_resolver(
    chain: &SubstitutionChain,
    given: &HashMap<String, Value>,
    resolver: &IdentResolver<'_>,
) -> Option<HashMap<String, Value>>;
```

The runtime hook constructs the resolver from `self.enums`.

### 3. Gate expansion: `is_pure_assignment_body_with_enums`

- Now accepts `Keyword::Fsm` in addition to Claim/Schema/Type.
- Accepts Memberships whose type passes an `is_enum: &dyn Fn(&str) -> bool`
  predicate. Runtime passes one that consults the enum registry.

### 4. Enum-typed `given` pinning in `evaluate` + `classify_components`

Both functions had a `_ =>` "type mismatch" warning when the given
value was an enum. Added the `(Var::EnumVar, Value::Enum)` arm in
both, calling `super::encode_ast::value_enum_to_datatype` to build
the Z3 datatype value and asserting equality.

(Couldn't fix `run_cached`'s same site because of lifetime constraints
— it takes `&'ctx Context` not `&'static`. Acceptable: `run_cached`
isn't the path `rt.query` uses for one-shot calls.)

### 5. Enum-typed component classification

`classify_components`'s "differs from model" check added a case for
`Var::EnumVar`: assert `ast._eq(model.eval(ast))` then negate. The
`_ =>` previously fell through to "unsupported," which forced
non-functional verdicts on every enum-containing component.

### 6. Filter `Var::EnumValue` from var_names

`populate_enum_variants` adds entries for every known enum variant
(`Init`, `Done`, `Mon`, ...) as `Var::EnumValue`. Those are runtime-
constant values, not user-declared variables — they were polluting
the `var_names` list passed to `decompose`, which then included them
in the components and confused everything downstream.

### 7. `EVIDENT_FUNCTIONIZE_TRACE=1` diagnostic

Optional env var that prints `[fz] HIT/MISS/...` lines per query and
detailed failure reasons (gate rejection, non-functional components,
chain-extraction failure, eval failure). Doesn't appear in normal
operation; useful for debugging gate-expansion attempts.

## Bench results

`runtime/tests/functionize_match.rs::match_dispatch_bench`:

```
Match-dispatch bench (2000 iter):
  Z3 query:    47.90 μs/call
  Native (fz):  2.48 μs/call
  Speedup:    19×
```

Tests verify:
- `match_dispatch_compiles_natively`: Z3 and native produce identical
  bindings for `state = Init` → `state_next = Done`.
- `match_dispatch_state_done_stays_done`: `state = Done` → `state_next = Done`.
- `match_dispatch_bench`: native faster than Z3 (asserted).

## Gate coverage delta

Run `cargo run --release --example probe_gate` to confirm:

```
Before Round 2: 123/460 claims pass (27%)
After Round 2:  243/460 claims pass (53%)
```

The doubling comes mostly from accepting `Keyword::Fsm` (many test
files use FSM bodies) and enum-typed memberships (which unlocks
state-machine-shaped claims).

## What's still rejected (Round 3+ work)

Inspecting probe_gate output for claims NOT passing:
- Memberships of user record types (`pos ∈ IVec2`, `aabb ∈ AABB`).
- Memberships of Seq/Set/Composite types.
- Passthroughs (`..Level`, `..LevelConstants`).
- ClaimCalls (subclaim invocations).
- Bodies with non-equality constraints (`∀`, `∃`, `⇒`, comparisons).

Mario's FSMs are blocked by ALL of these. To make Mario's display
function-shaped, Round 3+ needs at least:
- User-record memberships (with field expansion in eval_expr)
- Field access in expressions (`player.pos.x`)
- Passthrough handling (inline the passthrough'd body's equalities)

## Round 3 candidates

Top contenders given current state:
1. **User-record Membership + Field access** (estimated 3-5d). Unlocks
   Mario's `pos ∈ IVec2` style, `player.pos.x` lookups.
2. **Translation cache** (1-2d). Doesn't extend coverage but speeds up
   the cache-miss path on every query.
3. **Per-FSM warm solver with assumption pinning** (3-5d). Big win on
   per-tick performance.

Round 3 pick: **user-record Membership + Field access**. Directly
extends what the function-izer can compile; compounds with Round 2's
Match support.
