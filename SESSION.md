# Session RR — Child-FSM effects percolate up to the parent (no child-side dispatch)

> Read this first, then `CLAUDE.md`, then in order:
> `runtime/src/runtime/nested.rs` (`resolve_runs` — tier-3 child eval),
> `runtime/src/effect_loop/nested.rs` (the child run exec — where effects
> would be dispatched), the spot in LL's work that **rejects
> effect-emitting child FSMs at load** (the v1 restriction you're
> replacing), `runtime/src/effect_dispatch.rs` (how effects are normally
> dispatched), `examples/test_35_run_fsm.ev` (the run() baseline). Then build.

## What we want

Implement the design decision: **a child FSM must not emit effects
itself; the effects it solves for percolate up to the parent.** Today
(LL v1) an effect-emitting child is *rejected at load*. Replace that
restriction with **capture-and-return**: the child may solve for effects,
but during its run they are **not dispatched** — they are collected and
**returned to the parent**, which incorporates them into its own effects.

This keeps the child a **pure function** of its inputs: `run(F, init)`
yields `(final_state, collected_effects)` with **no side effects during
the child run**. Referential transparency is preserved (same inputs →
same result *and* same collected effects), and the parent stays the sole
authority on what actually gets dispatched — it can even discard the
child's effects if its constraints reject that solution.

## Acceptance criteria

1. `./test.sh` passes (cargo + 123 conformance). `test_35` / `test_36` /
   `test_37` still pass.
2. **A child FSM may now declare/solve `effects`** without a load-time
   rejection. During the child's tier-3 run, those effects are
   **captured, not dispatched** (nothing prints / no LibCall fires from
   inside the child run).
3. **The captured effects percolate to the parent**: they become
   available to the parent (e.g. appended to the parent's `effects`, or
   exposed as part of the child's returned value for the parent to place
   in its own `effects`). Pick the cleaner of those and document it. The
   parent's normal dispatch is what actually emits them.
4. **Ordering / dedup correctness:** effects emitted in child-tick order,
   then dispatched once by the parent — no double-dispatch, no
   dispatch-during-child-run. A test proves an effect a child "emits"
   appears in output exactly once, and only after the parent dispatches.
5. **Purity:** same `init` → same `(state, effects)`. A test asserts a
   re-run yields identical captured effects (the seed/referential-
   transparency invariant LL established still holds with effects).
6. `runtime/tests/run_fsm.rs` (or a new `nested_effects.rs`) covers:
   capture-not-dispatch, percolation to parent, order, single-dispatch,
   purity. Update the LL restriction note in
   `docs/design/nested-fsm-strategies.md` (effects now percolate, not
   rejected).

## Scope

| You may create / edit | You may NOT edit |
|---|---|
| `runtime/src/runtime/nested.rs`, `runtime/src/effect_loop/nested.rs` (capture mode + percolation) | `runtime/src/portable/subscriptions.rs`, `stdlib/passes/*` (QQ owns) |
| the validation that rejected effectful children (relax it) | `runtime/src/fsm_unroll/*` |
| `runtime/src/effect_dispatch.rs` ONLY if a capture hook is needed (additive) | `runtime/src/translate/*` (QQ may touch for a gap) |
| `runtime/tests/run_fsm.rs` or `runtime/tests/nested_effects.rs`, an `examples/test_38_*.ev` if useful | `docs/design/*` except the one nested-fsm-strategies status update |

## Approach

1. Find where LL rejects effectful children; understand the current run
   path in `effect_loop/nested.rs` — specifically where a child tick's
   effects would otherwise reach `effect_dispatch`.
2. Add a **capture mode** to the child run: instead of dispatching the
   child's per-tick effects, accumulate them into a `Seq(Effect)` (in
   child-tick order).
3. Return the accumulated effects alongside the final state from
   `resolve_runs`. Decide how the parent receives them (append to
   parent `effects` vs returned-value field) — cleanest wins, document it.
4. Ensure the parent dispatches them exactly once, after the child
   completes, in order.
5. Tests: capture-not-dispatch, percolation, order, single-dispatch, purity.

## Validation

```sh
cd /Users/danroblewis/evident-sessions/RR-effects-percolate
cargo build --release --manifest-path runtime/Cargo.toml
./test.sh
cargo test --release --test nested_effects -- --nocapture 2>&1 | tail   # or run_fsm
```

## Commit + push

```sh
git add -A && git status
git commit -m "feat: child-FSM effects percolate up to the parent (no child-side dispatch)

[Body: child run captures effects instead of dispatching them, returns
them to the parent which emits them once in order; replaces LL's
reject-effectful-child restriction; keeps the child a pure function of
its inputs (same init → same state AND same captured effects).]

./test.sh green."
git push -u origin session-RR-effects-percolate
```

## Background

- The decision: the child is a pure function; effects are part of what it
  *returns*, not something it *does*. The parent — which may reject the
  solution entirely — is the sole dispatch authority.
- This deepens LL's v1 (which just rejected effectful children) into the
  real semantics. Correctness baseline first; speed/tiers unaffected.
- Parallel: QQ (portable/subscriptions + stdlib) and SS (docs). Disjoint
  from your files (`nested.rs`, `effect_loop/nested.rs`).
- ~3-4 hours. Capture mode + percolation + the ordering/purity tests.
