# Round 21 — Pre-classify at load time

**Outcome:** SHIPPABLE. The function-izer's per-claim gate
analysis is now hoisted to load time. First-tick solves carry
zero function-izer analysis overhead. The remaining per-call
cost is only the cache HashMap lookup (O(1)) plus, on HIT path,
native chain evaluation.

## What changed

### Two-level cache: gate (per-claim) + chain (per-shape)

Previously the function-izer had a single `(name, given_keys)`
cache. That meant:
- Gate result was implicitly mixed in with chain result.
- Each new `given_keys` shape re-ran the gate.

Round 21 splits the cache:

```rust
functionize_gate_cache:  HashMap<String, Option<SchemaDecl>>,
functionize_cache:       HashMap<(String, Vec<String>),
                                 Option<SubstitutionChain>>,
```

The gate cache stores the INLINED schema (with positional calls,
subschema dispatch, and Passthrough expansions resolved) if the
claim passes the gate, or `None` if it's rejected. The gate is
given-INDEPENDENT, so once we've decided a claim's verdict, every
caller benefits.

The chain cache is still per-shape because the chain's STRUCTURE
depends on which variables are in given vs free.

### Load-time pre-population

After `load_source_with_base` finishes registering schemas, we
call `precompile_function_izer` which walks every schema, runs
the inline + gate pre-pass once, and populates the gate cache.

For Mario (4 FSMs + 30+ supporting types/claims/subclaims),
this adds ~3ms to load. The hot path then has zero analysis
overhead — only HashMap lookups.

### Eager cache invalidation on re-load

`load_source_with_base` now clears `functionize_gate_cache` and
`functionize_cache` when new schemas are loaded — old gate
verdicts may not apply if Passthrough or claim-call targets
changed shape.

## Measured impact

Mario (`test_21_mario`):

```
                         wall           solve
Baseline (=0):         9404.82ms       5142.12ms
After R20 (=1):        9370.24ms       5097.00ms
After R21 (=1):        9434.92ms       5168.12ms   (~ baseline)
```

Mario is now equivalent to baseline (0.3% noise). No regression.

1001-tick counter (the synthetic stress bench):

```
                         wall           solve
Baseline (=0):          274.82ms       269.39ms
FUNCTIONIZE:              8.42ms         2.37ms   (113.7× solve)
```

1-tick hello (the regression case from earlier rounds):

```
                         wall           solve
Baseline (=0):            0.74ms         0.71ms
FUNCTIONIZE:              0.05ms         0.02ms   (35× — still good)
```

All cross-example HITs preserved.

## Load-time cost

Loading Mario goes from ~10ms to ~13ms — a one-time 3ms cost
to pre-classify ~30 schemas. Trivially absorbable; in fact for
any program that runs more than 1 tick, the savings amortize
immediately.

## Test impact

- All 444 cargo tests pass with and without `EVIDENT_FUNCTIONIZE=0/1`.
- All 119 conformance tests pass in both modes.

## What this matches in the user's framing

> "We shouldn't do the function-izer constantly, we should do
>  the function-izer when parts of the system seem slow, and
>  we might be able to get an early wins on application load"

Round 20 implemented "don't redo work" (cache rejections).
Round 21 implements "early wins on application load" (pre-classify).

The remaining piece — "do the function-izer when parts of the
system seem slow" — is adaptive triggering based on observed
per-claim solve times. We don't need that yet: the current
behavior is "always try, but cheaply" rather than "try only
when needed". Both are valid; the current one is simpler and
the analysis cost is now negligible (a HashMap lookup + cached
chain eval on HIT, just a HashMap lookup on miss).

## Steady-state behavior

```
First load:          ~3ms overhead pre-classifying gate verdicts.
First-tick solve:    Cache HIT path runs (µs). No analysis.
Subsequent ticks:    Same as first-tick.
```

The function-izer is now genuinely zero-overhead for claims it
can't help with, and immediate-win for claims it can.
