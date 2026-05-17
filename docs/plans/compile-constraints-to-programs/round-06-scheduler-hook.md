# Round 6 — Scheduler-side function-izer hook

**Outcome:** PASS for the hook; the function-izer is now tried on
every scheduler tick. The gate still rejects Mario's FSMs (∀,
ClaimCall, ternary, non-equality), but for the right reason —
those need separate gate expansions, NOT scheduler integration work.

## The bug Round 5 hinted at

`rt.query` had the `EVIDENT_FUNCTIONIZE` hook, but the scheduler
doesn't use `rt.query`. It uses `rt.query_with_pins_and_given`.
**That method had no hook.** So `EVIDENT_FUNCTIONIZE=1` was a no-op
for the multi-FSM scheduler — the function-izer never got a chance
to fire on a Mario tick.

## The fix

`rt.query_with_pins_and_given` now checks `EVIDENT_FUNCTIONIZE=1`
and, when `pins` is empty (FSMs without state-pair enum pinning —
Mario's case, every FSM is `fsm name(world ∈ World)`), routes
through `try_functionize`. On miss, falls through to the existing
Z3 path as before.

When `pins` is non-empty (FSMs WITH state pairs — `fsm hello(state ∈
HelloState)` etc.) the v1 still falls through to Z3. That case
needs a Datatype → Value::Enum converter to re-pack the state into
`given` for the function-izer; that's its own piece of work.

## Trace evidence

```
EVIDENT_FUNCTIONIZE=1 EVIDENT_FUNCTIONIZE_TRACE=1
  evident effect-run examples/test_21_mario/main.ev

[fz] SDL_Window: rejected by gate
[fz] level_gen:  rejected by gate
[fz] game:       rejected by gate
[fz] keyboard:   rejected by gate
[fz] display:    rejected by gate
[fz] level_gen:  rejected by gate
...
```

The function-izer is now correctly INVOKED for every per-tick FSM
solve. It rejects them all at the gate (gate-level refusal,
before classify_components even runs). The reasons:

- Memberships of types we don't accept (Seq, SDL_Window, etc.)
- ∀-quantified body constraints
- ClaimCall body items (subclaim invocations)
- Ternary expressions inside body Constraints' RHS — those are
  parsed as `BodyItem::Constraint(Expr::Ternary(...))` when the
  body is something like `frame ≥ 240 ? ... : ...`, not as a
  pure Eq.

## What this round produced

- Real scheduler-side integration. Future rounds expanding the
  gate now flow automatically to the per-tick FSM path.
- A clear diagnostic: Mario's gate-refusal is about CONSTRAINT
  SHAPES in the body, not about pinning or runtime context.

## Round 7 candidates

The gate-rejection reasons listed above are each a discrete piece
of work. In rough priority:

1. **Accept `Eq(lhs, Match(...))`** — display has lots of
   `eff = match world.state { Init ⇒ Println(...) | ... }`. The
   gate currently sees this as Eq-with-Match-RHS, which it accepts
   (Match isn't a top-level Constraint). But the body might have
   OTHER constraints rejected by the gate. Need targeted look.
2. **Accept ternary RHS in Eq constraints** — `BodyItem::Constraint(
   Expr::Binary(BinOp::Eq, _, Box::new(Expr::Ternary(...))))`. Should
   already be accepted (it's still an Eq at the top level). If
   we're rejecting it, that's a bug in the walker.
3. **Accept ClaimCall** — when the called claim is pure-assignment
   itself, we can inline its body. Similar to Passthrough.
4. **Accept ∀ over Range with literal bounds** — biggest unlock,
   biggest work.
5. **Accept Seq-typed Memberships** — `effects ∈ Seq(Effect)`. Need
   Seq value handling in eval_expr.

Round 7 pick: investigate WHY each Mario FSM is rejected
specifically, then attack the simplest blocker.
