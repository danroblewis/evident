# Round 18 — ∀-over-Seq + Passthrough flattening

**Outcome:** STRUCTURAL CHANGE. The function-izer's `expand_foralls`
now unrolls `∀ x ∈ seq` and `∀ (a, b, …) ∈ coindexed(s1, s2, …)`
using statically-known seq lengths. Combined with hoisted
Passthrough flattening (so `..Level` brings `#platforms = 4` into
scope at gate time), Mario `game` and `display` both move past
the `Forall (non-static bounds)` rejection.

## What changed

### `try_unroll_forall_seq(e, lengths)`

Mirrors `try_unroll_forall_range` for two new patterns:

1. **`∀ x ∈ seq`** — `seq` is an Identifier with a known length N.
   Substitutes `x` with `Index(seq, i)` for i in 0..N.

2. **`∀ (a, b, …) ∈ coindexed(sa, sb, …)`** — multi-binding form.
   Every operand must be an Identifier with a known length; all
   lengths must match. Substitutes each bound name with
   `Index(seqK, i)` for the corresponding seq.

### `collect_seq_lengths(body)`

Walks the body for `#seq = literal` (or `literal = #seq`)
constraints, producing a `HashMap<seq_name, length>`. Used as
input to the ∀ unroller.

### `expand_foralls_with_lengths`

Refactor: the public `expand_foralls` now collects lengths first,
then walks the body trying both range and seq unrollers.

### Passthrough flattening hoisted into `inline_positional_calls`

Previously, `..Level` Passthroughs were expanded only inside
`try_extract_one_chain`'s body construction. The gate's
`collect_seq_lengths` couldn't see Level's `#platforms = 4`
because Level's body items hadn't been merged in yet.

Round 18 adds `flatten_passthroughs(body, claim_lookup)` as the
first phase of `inline_positional_calls`. Every `..ClaimName` is
replaced by the referenced claim's body items inline, with a
visiting set to prevent cycles.

### Gate accepts ∀-over-Seq

`gate_diagnostics` now calls `try_unroll_forall_seq` as a fallback
when `try_unroll_forall_range` returns None.

## Mario rejection progress

```
                Before R18                          After R18
display:    Forall (non-static bounds)         Forall body: body Call win.draw_rect
game:       Forall (non-static bounds)         Forall body: non-Eq Binary op Implies
keyboard:   (slow-path classify)               (slow-path classify, unchanged)
level_gen:  non-Eq Binary op Implies           non-Eq Binary op Implies
```

Both `display` and `game` are past the ∀ unrolling. New blockers:
- **display**: the unrolled ∀ body has `win.draw_rect(...)` calls.
  These need subschema inlining INSIDE the ∀ body. Currently
  `inline_positional_calls` only visits top-level Constraint Calls,
  not ones nested inside ∀.
- **game**: the unrolled ∀ body contains `Implies` constraints
  (e.g. `on_ground ⇒ vy = jump_strength`). Implies handling is
  Round 19's target.
- **level_gen**: still blocked on `Implies`. Same fix.

## Test impact

- All 444 cargo + 119 conformance pass in both modes.
- Cross-example HIT count unchanged (no demo crosses the new
  threshold without further work).

## Round 19 candidates

1. **Inline subschema calls inside ∀ bodies.** `inline_positional_calls`
   currently only handles top-level Call constraints. Extend it
   to recurse into ∀ bodies (and other compound Exprs) so the
   unrolled `win.draw_rect(...)` calls get expanded.

2. **`Implies` as guarded substitution.** `cond ⇒ var = expr`
   becomes `var = (cond ? expr : <free>)`. Soundness is tricky
   when the free branch matters; usually it only matters when the
   guard captures one-shot init patterns (`state.step = 0 ⇒ Init`)
   where the var is consumed by other code that ALSO guards on
   the same condition. For Mario level_gen this is the case.

Recommend doing #1 first (smaller, unblocks display) then #2
(harder soundness story, unblocks game and level_gen).
