# Round 16 — World carry-through (partial)

**Outcome:** STRUCTURAL CHANGE. The fast path now skips
`world_next.X` Memberships that have no substitution in the
body — these are FSM "carry-through" outputs the scheduler keeps
unchanged from the prior tick. No new HITs yet; the deeper
problem is the slow-path classifier still sees other components
as non-functional, and the fast path is also returning None for
reasons not yet diagnosed.

## What changed

`try_extract_one_chain` adds a pre-pass: collect which names
appear as the LHS or RHS of an `Eq` body Constraint (i.e., have
some substitution candidate). When walking Memberships to build
the target set, skip any name that starts with `world_next.` AND
isn't in that has-substitution set. The scheduler's world merger
keeps the prior tick's value for unwritten fields, so these
shouldn't be chain targets.

This matches the runtime's existing `subscriptions::world_access_sets`
inference at semantic level (FSMs only write what they explicitly
assign).

## Why it doesn't yet land Mario keyboard

Even with the fast-path skip, the slow-path classifier (called
when fast path returns None) still reports the same non-functional
components on keyboard, including `world.keys.x` and `world.keys.y`.
Those world reads ARE in `given` (via the scheduler's
`world_snapshot`), so it's unclear why classify treats them as
non-pinned. Two hypotheses:
- The scheduler's world-rewrite somehow strips them.
- `classify_components` treats reads differently than writes.

Diagnosing requires more instrumentation than this round's scope.
Mario remains gate-passing but classify-rejecting.

## What this preserves

- Tests still green in both modes.
- Cross-example HIT count unchanged.
- The infrastructure is correct: when the fast path DOES extract
  a chain, world_next carry-through is now part of the model.

## Round 17 candidate

Drop the diagnostic-only world_next change in favor of a focused
investigation: instrument the slow-path classify to print why
exactly `world.keys.x` (a given-pinned value) shows up as a
non-functional component. Either find the bug, OR move to a
different track entirely.

Alternative: pivot to attacking the toposort criterion (PLAN
goal #2). Dispatcher's self-hosted toposort is at 521ms — a 50×
speedup target. That workload may already fit the function-izer's
shape and be a simpler win than chasing Mario.
