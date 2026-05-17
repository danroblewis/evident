# Round 19 — Target collection from Constraint LHS + pinned-membership lifting + cache-on-extract

**Outcome:** STRUCTURAL CHANGES. Three independent fixes to the
function-izer's fast-path foundation. Mario `keyboard` now passes
the chain extractor; eval still fails per-tick (cross-FSM
`_world.tick` propagation is the remaining gap, deferred to
Round 20).

## What changed

### 1. Collect targets from Constraint LHS Identifiers (with root check)

`try_extract_one_chain` now extends its target var set with every
dotted Identifier that appears as the LHS or RHS of a body
`Eq` Constraint, IF the Identifier's root (first dotted segment)
is a declared Membership in scope.

Why: Mario keyboard's body contains `world_next.keys.x = ternary`.
`world_next.keys.x` isn't a top-level Membership in the schema —
only `world_next ∈ World` is. The dotted leaves arise from World
expansion at translation time, which the fast path doesn't run.
Without explicit collection, `world_next.keys.x` wasn't a chain
step; the constraint became a "check" that couldn't evaluate.

The root-check prevents bare-name leaks (the
`cli_query_without_infer_types_fails_for_undeclared_vars` strict
test): `msg = "hello"` doesn't add `msg` as a target because
`msg` isn't in any Membership.

### 2. Skip composite-typed Memberships without substitution

`try_extract_one_chain` previously included EVERY non-given
Membership as a target. `world ∈ World` and `world_next ∈ World`
got added; they have no substitution → `extract_chain_xl`
returned None.

Round 19 broadens the skip: any non-given Membership with no
`name = expr` substitution AND a non-primitive, non-enum,
non-Seq/Set type. These are "container" decls whose leaves are
the real substitution targets.

### 3. Lift pinned Memberships into Eq constraints

`v ∈ IVec2(-800, 540)` and `win ∈ SDL_Window (title ↦ "Mario",
width ↦ 640, height ↦ 480)` encode field pinning via the `Pins`
variant, not as separate body Constraints. The Z3 path emits
equality assertions for each pin internally; the function-izer
fast path needs the same.

`expand_pinned_memberships(body, claim_lookup)` synthesizes
`Constraint(Eq(Identifier("v.x"), Int(-800)))` etc. before chain
extraction sees the body. Named pins use the slot name directly;
positional pins look up the type's field declaration order via
`claim_lookup`.

### 4. Cache the chain even when first-call eval fails

Previously, when the first-call eval failed (e.g., tick 0 has
empty `last_results` so `match last_results[i]` can't evaluate),
the cache stored `None` — permanently disabling the fast path for
that schema. Subsequent ticks with valid data still ran the slow
path.

Round 19 caches `Some(chain)` regardless of first-call eval
outcome. The chain's STRUCTURE is stable per schema; only the
DATA varies per tick. If eval fails on a given tick, return None
(fall through to Z3) without poisoning the cache.

## Mario rejection progress

```
                Before R19                             After R19
keyboard:   classify rejection (fast-path None)   chain extracts (12 steps),
                                                  eval fails on tick 0 (_world.tick
                                                  not yet in given); chain cached
                                                  for tick 1+, but eval still fails
                                                  there due to cross-FSM `_world`
                                                  propagation gap.
```

Display, game, level_gen unchanged — their blockers are deeper
(∀-internal subschema calls, Implies).

## Test impact

- All 444 cargo tests pass in both modes.
- All 119 conformance tests pass in both modes.
- Cross-example HIT count unchanged (Mario keyboard still doesn't
  HIT due to the per-tick eval issue described above).

## Round 20 candidate

Diagnose `_world.tick` propagation: the scheduler's `prev_values`
appears to capture only the current FSM's writes, not the
merged world snapshot. So in keyboard FSM, `_world.tick = ?` —
keyboard doesn't write `world.tick`, so its `prev_values` has
no `tick` entry, so `_world.tick` isn't in `fsm_view`.

The fix: when populating `fsm_view`'s `_world.X` entries, mirror
ALL world snapshot fields from the prior tick, not just this
FSM's own writes. The scheduler likely already does this via the
shared world snapshot mechanism; the function-izer just needs to
SEE those values in `given`.

Alternative: defer Mario, attack `Implies` handling (unblocks game
+ level_gen at once).
