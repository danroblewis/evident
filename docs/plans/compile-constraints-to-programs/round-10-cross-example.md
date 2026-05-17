# Round 10 — Cross-example fire-rate measurement

**Outcome:** STARK FINDING. Across 17 non-SDL example files, the
function-izer fires ZERO HITs at runtime. Two distinct reasons —
both architectural, not implementation bugs.

## The measurement

`/tmp/cross_bench.sh` runs each example with
`EVIDENT_FUNCTIONIZE=1 EVIDENT_FUNCTIONIZE_TRACE=1` and counts
`[fz] HIT` vs `[fz] rejected` per FSM tick. Result:

```
Example                          | HIT | MISS | HIT/total
test_01_hello.ev                  |   0 |    0 | —      ← hook didn't fire
test_02_counter.ev                |   0 |    0 | —
test_03_seq_chain.ev              |   0 |    0 | —
test_04_parse_int.ev              |   0 |    0 | —
...   (all examples)              |   0 |    0 | —
```

ZERO trace output for all 17 examples. Examined directly:

## Reason 1: state-pair FSMs bypass the hook

Hello and many other examples use `fsm name(state ∈ HelloState)`.
The scheduler builds a `pins: &[(&str, Datatype<'static>)]` with
the state pinned as a Z3 Datatype. My function-izer hook in
`query_with_pins_and_given` only fires when `pins.is_empty()`.
So **every state-pair FSM bypasses the function-izer entirely.**

The fix would convert each pin's Datatype back to `Value::Enum`
using `extract_enum_value` (or a Z3-simplify-based variant) and
merge into the `given` map before calling `try_functionize`. ~1
day of work but requires careful enum-registry plumbing.

## Reason 2: examples I tested earlier hit gate refusals

Mario and walker (test_22) DO get the hook (no state pin). But
they reject at the gate for reasons documented in Rounds 8-9
(SDL_Window, ∀-over-Seq, Implies, _var unpinning).

Round 9's measurement showed identical Mario perf with/without
the flag — confirming the hook fires but every FSM bails.

## The plan-level honest interpretation

After 10 rounds:
- Function-izer infrastructure: complete and sound.
- Synthetic benchmarks (Pair, Step, HelloStep): 19-242× speedups
  on hand-written test claims.
- Real example workloads: 0 actual speedup. Not one.

This isn't a regression — it's that the foundation is GENERAL but
the COVERAGE is narrow. Each new gate widening unlocks one or two
specific patterns. Mario uses patterns we haven't built yet
(∀-over-Seq, ⇒-Implies, FFI types). Hello uses patterns we DID
build (Match dispatch) but bypasses via the state-pin issue above.

## Round 11 candidates — three honest options

1. **Fix state-pin bypass** (~1 day): unlock hello/counter/etc.
   Tests that already pass with EVIDENT_FUNCTIONIZE=1 should show
   real speedups (Match dispatch was the Round 2 19× synthetic
   bench — it'd land on test_01_hello directly).
2. **∀-over-Seq unrolling** (3-5 days): the Mario `game` blocker.
   Needs Field/Index resolution in eval and seq-length resolution
   from pinned cardinality.
3. **Component-level partial compilation** (~2 weeks): the
   architecturally-cleanest path to Mario. Compile each component
   independently; route function-shaped components native, leave
   search-shaped to Z3 with augmented given.

Round 11 pick: **option 1, fix state-pin bypass**. Smallest, most
likely to produce a SHIPPABLE real-world speedup. If hello goes
from 30μs to 3μs per query, we have demonstrable runtime value
for the first time after 10 rounds.

The plan continues, but Round 11 also flags a hard truth: the
"Mario is faster" success criterion from PLAN.md is more than
two rounds away. Each option above pushes closer; none lands the
prize this iteration. Honest accounting matters.
