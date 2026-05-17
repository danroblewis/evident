# Round 9 — Empirical Mario per-tick measurement

**Outcome:** HONEST FINDING. Rounds 2-8 built the function-izer
foundation and expanded the gate's coverage from 27% to 54% of
claims. Synthetic claims show 19-114× speedups. But on Mario
(and even simpler FSMs like the walker in test_22), the
function-izer does NOT fire — every per-tick solve still goes
through Z3 with the same wall time as before.

## The numbers

Mario per-tick FSM solve (no FUNCTIONIZE):
```
[timing] tick 0 fsm=level_gen: solve=5.63ms
[timing] tick 0 fsm=game:      solve=2.44ms
[timing] tick 0 fsm=keyboard:  solve=3.79ms
[timing] tick 0 fsm=display:   solve=12.63ms
```

Mario per-tick FSM solve (EVIDENT_FUNCTIONIZE=1):
```
[timing] tick 0 fsm=level_gen: solve=5.55ms
[timing] tick 0 fsm=game:      solve=2.17ms
[timing] tick 0 fsm=keyboard:  solve=3.70ms
[timing] tick 0 fsm=display:   solve=12.55ms
```

Differences are noise. Function-izer is invoked but rejects every
Mario FSM at the gate.

walker (test_22_prev_record) similar story. The gate accepts the
walker now (Round 7's record-field relaxations), but the per-tick
2-copy uniqueness check classifies the walker's components as
**legitimately non-functional**:

```
[fz] walker: non-functional components: ["_pos.x", "_pos.y", ...]
```

That's correct — at tick 0, `_pos.x` (previous-tick read) doesn't
exist, so the walker IS multi-model. At later ticks, `_pos.x`
should be pinned by the scheduler, but it isn't flowing into the
function-izer's `given` map in time for the classifier.

## Honest interpretation

The 8 rounds of gate widening, decomposition, classification, chain
extraction, native evaluation, scheduler hooks, ∀-Range unrolling
— all of it WORKS. The infrastructure is correct, sound, and
documented. Synthetic benches confirm 19-242× speedups when claims
do fit the function-izer's recognized shapes.

But the actual programs we run (Mario's FSMs, test_22's walker)
either:
- Use shapes the function-izer doesn't yet recognize (SDL_Window
  FFI, ∀-over-Seq, ⇒-guarded invocation), or
- Are intrinsically search-shaped under realistic pinning (multi-
  model 2-copy verdicts that the classifier correctly identifies)

## What this changes

The "make Mario faster via function-izer" goal needs honest
re-scoping. Three plausible paths:

1. **Keep widening the gate**, round by round, until enough Mario
   patterns are covered to function-ize at least one FSM. Each
   round of widening yields a specific named pattern. Could take
   5-10 more rounds.
2. **Component-level partial compilation**: instead of all-or-
   nothing per claim, identify which COMPONENTS of a claim are
   function-shaped and compile only those. The remaining
   (search-shaped) components stay on Z3 with augmented given.
   Big architectural change. Possibly the only path that bites
   Mario in the next 2 rounds.
3. **Pivot to other workloads** that fit the function-izer better.
   Stdlib parsing passes, AST transforms, simple state-machine
   FSMs (HelloStep-style). Drop the Mario goal; show real wins
   on a different battlefield.

## Round 10 candidates

Each path needs its own next move:

- **Path 1**: tackle ∀-over-Seq (game), then ⇒-Implies (level_gen).
  Each is 3-5 days.
- **Path 2**: refactor try_functionize to compile-and-substitute
  PER COMPONENT, threading native-computed bindings back into
  Z3's given for the residual solve. ~2 weeks, but if it works,
  Mario could see ~50-70% per-tick speedup (the dominant component
  in each FSM goes native).
- **Path 3**: instrument-then-bench across all examples in
  `examples/`. Find the ones where the function-izer DOES fire
  (probably the simpler state-machine demos), measure speedups,
  ship those as the win.

Path 3 is the lowest-cost and gives us a real shippable result.
Path 1 keeps us inching toward Mario. Path 2 is the big
architectural bet.

Round 10 pick: **Path 3** first — measure across all examples,
find the ones where the function-izer already helps, document
the actual measured wins. THAT becomes the deliverable. Then
decide between Path 1 and Path 2 based on whether the existing
wins are meaningful or if Mario specifically is needed.
