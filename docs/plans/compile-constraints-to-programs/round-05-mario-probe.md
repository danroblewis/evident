# Round 5 — Probing Mario after Rounds 2-4 expansions

**Outcome:** DIAGNOSTIC. Built `probe_mario` to check whether the
function-izer can now compile any of Mario's FSM bodies. Finding:
none do, but for an *interesting* reason — Mario's FSMs are
**intrinsically search-shaped under empty given**. Even with the
expanded gate, the 2-copy uniqueness check fails: multiple valid
models exist for the multi-var components.

## What probe_mario shows

```
level_gen      :  83 components ( 82 singletons),  1 multi-var ( 0 functional)
game           :  68 components ( 66 singletons),  2 multi-var ( 0 functional)
keyboard       :  77 components ( 73 singletons),  4 multi-var ( 0 functional)
display        :  89 components ( 86 singletons),  3 multi-var ( 0 functional)
```

Each FSM has 1-4 multi-variable components and 60-90 singletons.
Every multi-var component is classified non-functional — Z3 finds
another model differing on its variables, so the function-izer
correctly refuses.

## Why this is the right answer

Calling `classify_components` with empty given means EVERY variable
is free. Mario's `display` has variables like:
- `_world.player.pos.x` (read from previous tick — would be given)
- `frame` (computed from `_frame` — would be derived)
- `sky_eff`, `clear_eff`, `plat_effs[0..3]` (effect outputs)

With nothing pinned, Z3 can pick any starting state. The 2-copy
check rightly says "not function-shaped" — many valid models.

The scheduler pins state-relevant given each tick. In that real
context, `display` IS function-shaped (one set of effects per
input world state). But our static `probe_mario` doesn't pin
those — it can't, because there's no concrete previous-tick state.

## Path forward

Two real opportunities:

1. **Hook the function-izer INTO the scheduler.** When run_with_ctx
   solves an FSM per tick, the given DOES have realistic values.
   If the function-izer were invoked from inside the multi-FSM
   scheduler with those givens, classification would happen with
   the real input partition. Mario's `display` likely becomes
   function-shaped there.

2. **Add finite-range ∀ unrolling.** Mario's FSMs have
   `∀ i ∈ {0..3} : plat_effs[i].x = ...` patterns. Even with realistic
   given, the gate currently refuses Forall expressions outright.
   Unrolling at chain-extract time would let those bodies flow
   through.

## Round 6 candidates

The two paths above are both valuable but separately scoped:

- **Run-inside-scheduler hook** (2-3d): wire `try_functionize` into
  the per-tick solve in `run_with_ctx`. May immediately unlock
  Mario's per-tick speed without any new gate work, since real
  given values are already there. Lowest-risk biggest-payoff move.
- **Finite-range ∀ unrolling** (3-5d): expand the gate + extractor
  to handle `Forall(var, Range(lit, lit), body)` by unrolling.

Round 6: scheduler hook first. Measure. If Mario gets faster,
ship. If not (the gate still refuses for other reasons), expand
the gate next.

## Code

`runtime/examples/probe_mario.rs` — loads Mario, runs
`classify_components` per FSM, reports component counts.

Run with `cargo run --release --example probe_mario`.
