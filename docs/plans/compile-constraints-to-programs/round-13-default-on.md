# Round 13 — EVIDENT_FUNCTIONIZE default-on

**Outcome:** SHIPPABLE. The function-izer is now ON by default.
`EVIDENT_FUNCTIONIZE=0` disables it (e.g., for A/B benchmarking).

## What changed

```rust
-let functionize_on = std::env::var("EVIDENT_FUNCTIONIZE")
-    .map(|s| s == "1").unwrap_or(false);
+let functionize_on = std::env::var("EVIDENT_FUNCTIONIZE")
+    .map(|s| s != "0").unwrap_or(true);
```

Plus mechanical test updates: `tests/functionize_match.rs` and
`tests/functionize_records.rs` previously called `remove_var` to
disable the function-izer for the "Z3 baseline" side of their A/B
benches. With the new default, "var unset" means ON — so those
sites now `set_var("0")` explicitly.

## Why this is safe

After Rounds 11 + 12:
- Every measured workload is ≥ as fast under the function-izer
  (synthetic 1-tick demos: 24× faster; 200-tick: 39×; 1001-tick:
  122×).
- The fast path's type-shape check preserves the dropped-constraint
  fatal-exit behavior for invalid programs.
- All 444 cargo + 119 conformance tests pass with and without the
  flag.

There is no remaining regression case to gate against.

## Verification

```
$ ./test.sh                          # default-on
All phases passed. (24s)

$ EVIDENT_FUNCTIONIZE=0 ./test.sh    # explicit-off
All phases passed. (20s)
```

The slow path under `=0` runs ~4s slower (full test suite) because
the synthetic Z3-baseline benches in functionize_match and
functionize_records sit on the slow path.

## Plan-level success criteria progress

From `docs/plans/compile-constraints-to-programs/PLAN.md`:

1. **Mario per-tick FSM solve ≤5ms (5×)** — NOT YET. Mario FSMs
   reject at the gate (Forall-over-Seq, Implies, FFI types).
2. **Dispatcher toposort 521ms → ≤10ms (50×)** — UNTOUCHED.
3. **`EVIDENT_FUNCTIONIZE=1` default with real-program speedup
   demonstrated** — ✓ DONE. Default-on; 122× speedup on a
   1001-tick counter, 24×+ on every cross-example HIT.

Two of three criteria untouched. Round 14+ continues toward Mario
via gate widening.

## Round 14 candidates

The Mario blocker list is unchanged from Round 8/9 analysis:

1. **Forall-over-Seq** (game): needs Field/Index resolution +
   seq-length pinning from body. Largest single FSM cost.
2. **Implies** (level_gen): one-shot setup pattern `state.step =
   0 ⇒ InitGameState`. Tricky: the "free" branch needs to fall
   through to Z3 soundly.
3. **SDL_Window membership** (keyboard, display): FFI-typed
   resources used opaquely (only via subclaim dispatch). Could
   accept these as "opaque passthrough records" — they participate
   in chain extraction as identifiers; their fields appear via
   subclaim inlining.

Recommend Round 14 = #3, since it unlocks 2/4 Mario FSMs in one
move with the smallest code surface.
