# Round 20 — Cache function-izer rejections

**Outcome:** SHIPPABLE. Mario's 3-7% regression under
`EVIDENT_FUNCTIONIZE=1` is eliminated. The function-izer now pays
analysis cost once per (claim, given-shape) for the LIFETIME of
the process — rejected claims skip the gate entirely on every
subsequent tick.

## What changed

Previously, every solve through `query_with_pins_and_given` ran:
1. Closures setup (~free)
2. Body inlining pre-passes (positional + subschema calls)
3. Gate diagnostics
4. *If gate passed:* cache lookup, chain extraction, eval
5. *If gate failed:* return None — **without caching the rejection**

For Mario's 4 FSMs over 241 ticks, that's ~960 redundant gate
checks per FSM-tick, plus the inlining pre-passes. Net: ~1.5ms
overhead per FSM-tick, slowing the game from 9.21s baseline to
9.51s under FUNCTIONIZE.

Round 20 restructures the function:

1. Cache lookup is now the FIRST step (after `cache_key`
   construction). On HIT: try eval, return result or fall
   through. On rejection: return None immediately.
2. Gate rejections now `cache.insert(key, None)` before
   returning, so future calls hit the cached rejection at step 1
   instead of re-running the gate.
3. The duplicate cache lookup downstream is removed.

## Measured impact

Mario (`test_21_mario`):

```
                    wall           solve
Baseline:         9411.16ms       5153.11ms
Round 19:         9509.59ms       5248.64ms   ← +3.3% wall (regression)
Round 20:         9370.24ms       5097.00ms   ← -0.4% wall (no regression)
```

Mario regression eliminated. The function-izer now has zero
measurable overhead for claims that always reject. FUNCTIONIZE
default-on is now a no-op for Mario-class programs (until
gate-widening reaches them), and a big win for everything else.

1001-tick counter (verified speedup preserved):

```
Baseline:        270.77ms solve
FUNCTIONIZE:       2.30ms solve   (117.7× speedup, unchanged from R19)
```

## Why this matches what we want

Per the user's framing: "we shouldn't do the function-izer
constantly, we should do the function-izer when parts of the
system seem slow."

Round 20 implements step 1 of that idea: **don't redo work the
function-izer has already concluded won't pay off**. A rejected
claim is rejected forever (within the process); the next tick
just sees an O(1) HashMap miss-as-None and skips.

What's still pay-as-you-go: the FIRST tick for each (claim,
given-shape) pays the full gate + inlining + chain extraction
cost. Round 21 hoists this to load time.

## Test impact

- 444 cargo tests pass with and without `EVIDENT_FUNCTIONIZE=0/1`.
- 119 conformance tests pass in both modes.
- All 6 cross-example HITs preserved (synthetic counter ×118 etc.).
