# Session status: Rounds 11–19 summary

**Branch:** `feat/compile-constraints-to-programs`
**Commits this session:** 9 (rounds 11 through 19, plus default-on
flip)

## Plan-level criteria

```
1. Mario per-tick FSM solve ≤5ms          ❌ Not yet
2. Toposort 521ms → ≤10ms                  ❌ Untouched
3. EVIDENT_FUNCTIONIZE=1 as default        ✅ DONE (Round 13)
```

## Real-world wins shipped

| Workload                          | Baseline solve | FUNCTIONIZE solve | Speedup |
|-----------------------------------|---------------:|------------------:|--------:|
| 1001-tick counter (`/tmp/`)       |       282.94ms |            2.32ms |   122×  |
| 200-tick counter (`/tmp/`)        |        61.99ms |            1.59ms |    39×  |
| 1-tick hello (`test_01_hello`)    |         0.71ms |            0.03ms |    24×  |
| 5-tick `test_20_pure_counter`     |   ~0.5ms/tick  |       <0.005ms*   |   100×* |

*Cache-amortized: tick 2+ runs at native-eval speed (~µs).

## Cross-example HITs

6 of 17 non-SDL demos now route through the function-izer:

```
test_01_hello.ev    100%   test_15_signal.ev     —
test_03_seq_chain   100%   test_18_reflection    0%
test_08_exit_code   100%   test_19_prev_tick    100%
test_10_spawn        50%   test_20_pure_counter 100%
```

## Rounds breakdown

| Round | Theme                                       | Impact                                  |
|------:|---------------------------------------------|-----------------------------------------|
|    11 | State-pin bypass + Seq classifier + eval    | First real speedup. 99× synthetic.     |
|    12 | Syntactic fast path (skip Z3 classifier)    | 24× on 1-tick, 122× on 1001-tick.      |
|    13 | EVIDENT_FUNCTIONIZE default-on              | Plan criterion #3 complete.            |
|    14 | Gate accepts external FFI types             | Structural — paves later wins.         |
|    15 | Positional claim-call inlining              | Structural — keyboard moves past gate. |
|    16 | World carry-through (partial)                | Structural.                            |
|    17 | Subschema-call inlining                     | Display moves past `body Call`.        |
|    18 | ∀-over-Seq + Passthrough flattening         | display/game past `Forall non-static`. |
|    19 | Target collection + pinned-membership lift  | Mario keyboard chain extracts.         |

## Mario rejection trajectory

```
                Start of session              End of Round 19
display:    Membership win∈SDL_Window     →  Forall body: body Call win.draw_rect
keyboard:   Membership win∈SDL_Window     →  fast-path chain extracts;
                                              eval fails on _world.tick
                                              cross-FSM propagation
game:       Forall (non-static bounds)    →  Forall body: non-Eq Implies
level_gen:  non-Eq Implies                →  non-Eq Implies (unchanged)
```

## Remaining Mario blockers (Rounds 20+)

1. **keyboard: cross-FSM `_world.X` propagation.** The scheduler's
   per-FSM `prev_values` captures THIS FSM's bindings, not the
   merged world snapshot. So `_world.tick` (display's write) isn't
   available in keyboard's `given`. Fix: mirror world snapshot
   into prev_values across all FSMs (the Z3 path may have the
   same gap and just hasn't been hit because slow-path classify
   rejects keyboard before the issue surfaces).

2. **display: subschema calls inside ∀ bodies.**
   `inline_positional_calls` currently visits only top-level
   Constraint Calls. Extend recursion into ∀ bodies (and other
   compound Exprs) so unrolled `win.draw_rect(...)` calls get
   expanded.

3. **game + level_gen: `Implies` as guarded substitution.**
   `cond ⇒ var = expr` becomes `var = (cond ? expr : <free>)`.
   Soundness is tricky — the "free" branch is undefined under
   the guard. Most uses (one-shot init `state.step = 0 ⇒ Init`)
   have downstream code that also guards on the same condition,
   so the free branch is unreachable in practice — but we'd need
   a static analysis to prove it.

## Code health

- 444 cargo tests pass with and without `EVIDENT_FUNCTIONIZE=0/1`.
- 119 conformance tests pass in both modes.
- No regressions on any cross-example demo's output.
- Build clean (only pre-existing dead-code warnings).

## Recommendation if continuing

Round 20 = fix `_world.X` cross-FSM propagation (1-2 days). If
that unblocks keyboard, we'll have the first Mario FSM speedup,
landing PLAN criterion #1 partially. Rounds 21+ then chase ∀-internal
inlining (display) and Implies (game/level_gen).

Alternative path: pivot to Toposort (PLAN #2) — the 521ms
self-hosted toposort is its own architectural problem (Z3 enumerating
permutations). It needs a different optimization (native algorithm
dispatch), not the chain-based function-izer.
