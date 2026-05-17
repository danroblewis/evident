# Round 11 — State-pin bypass fix + real speedup

**Outcome:** SHIPPABLE. After 10 rounds with ZERO measured speedup
on any real example, Round 11 delivers a 99.5× solve speedup and
34.7× wall speedup on a 1001-tick counter, and 6 of 17 cross-example
demos now HIT the function-izer at runtime.

## What changed

### 1. Scheduler surfaces state Value in `given`

`runtime/src/effect_loop.rs::run_with_ctx` already tracks each FSM's
state in two parallel forms — `current_state: Option<Datatype<'static>>`
(passed to Z3 via `pins`) and `current_state_v: Option<Value>` (the
Value form used for the multi-FSM `world` view). The function-izer's
hook in `query_with_pins_and_given` previously skipped any call with
non-empty `pins`. We now also insert `current_state_v` into `fsm_view`
before calling, so the function-izer's `given` map sees the pinned
state value even when the scheduler also pinned the Datatype.

The `pins.is_empty()` guard in
`runtime/src/runtime.rs::query_with_pins_and_given` is gone — the
function-izer fires whenever it's enabled. Pinned Datatypes become
redundant equality assertions, which Z3 absorbs without overhead.

### 2. Classifier handles Seq/Set vars

`runtime/src/translate/eval.rs::classify_components` used to treat
any non-primitive `Var` as "unsupported, non-functional". A Seq's
underlying Z3 representation is `Array<Int → T> + Int (len)`, and
naive `arr ≠ model_arr` is trivially SAT (the array can disagree at
indices outside [0, len)). The new check encodes the in-range
existential:

  `len ≠ model_len ∨ ∃k ∈ [0, len). arr[k] ≠ model_arr[k]`

via a fresh `Int::fresh_const(ctx, "fz_k")` constrained by `0 ≤ k <
len` plus `select(arr, k) ≠ select(model_arr, k)`. Z3 treats the
fresh const existentially in the assertion context.

Sets get plain `set ≠ model_set` (Set equality in Z3 is structural).

### 3. eval_expr handles Seq/Field/Index/Match/Call

Added handlers in `functionize.rs::eval_expr`:
- **`SeqLit(items)`** — evaluates each item, classifies into
  `Value::SeqInt`/`SeqBool`/`SeqStr`/`SeqEnum` by first-element type.
  Empty `⟨⟩` returns None (declared-type info not plumbed here),
  falling through to Z3 — affects ~1 test that uses `s = ⟨⟩`.
- **`Call(name, args)`** — constructor invocations like
  `Println("hi")` and `Exit(0)`. Evaluates args, then dispatches
  to a new `CtorResolver` that looks the variant up in the enum
  registry and builds `Value::Enum`.
- **`Index(target, idx)`** — `seq[i]`. Selects out a typed element
  from any of the Seq* Value variants.
- **`Cardinality(target)`** — `#seq` and `#set`.
- **`Field(target, name)`** — composite field access (folded
  identifiers handle the common case via env; this handles
  `seq[i].field`).
- **`Matches(scrut, pattern)`** — constructor recognizer; returns
  Bool from variant-name equality.

### 4. New `CtorResolver` plumbing

`functionize.rs` gains `pub type CtorResolver<'a> = dyn Fn(&str,
&[Value]) -> Option<Value> + 'a;` and an `evaluate_chain_with_resolvers`
entry point. The existing `evaluate_chain_with_resolver` stays as a
thin wrapper. `rt.try_functionize` builds both resolvers from the
enum registry.

## Measured speedups

`/tmp/bench_counter_1k.ev` — a 1001-tick counter using
`_count + 1`:

```
                    wall          solve     per-tick solve
Baseline:        291.54ms       285.56ms       0.285ms
FUNCTIONIZE=1:     8.40ms         2.87ms       0.003ms
Speedup:           34.7×          99.5×        99.5×
```

200-tick variant:

```
                    wall          solve     per-tick solve
Baseline:         56.10ms        54.86ms       0.273ms
FUNCTIONIZE=1:     3.56ms         2.24ms       0.011ms
Speedup:           15.8×          24.5×        25.2×
```

`/tmp/cross_bench.sh` — 17 non-SDL example demos:

```
Example                          | HIT | MISS
test_01_hello.ev                  |   1 |    0   ← first HIT on a real example
test_03_seq_chain.ev              |   1 |    0
test_08_exit_code.ev              |   1 |    0
test_10_spawn.ev                  |   2 |    2
test_19_prev_tick.ev              |   4 |    0
test_20_pure_counter.ev           |   4 |    0
```

For tiny 1-tick demos (hello, exit_code), FUNCTIONIZE is slightly
SLOWER because the classifier+extractor run on the first tick
doesn't amortize. Round 12 fix: move classification+extraction to
load time, so per-tick cost is just `evaluate_chain` lookup.

Demos that still REJECT: counter (parses `last_results[0]` with a
match arm whose body is `s`, which classifies OK; effects depends
on n_str, also classifies OK in isolation — but the joint 2-copy
check for both flags them non-functional somehow; possibly state-
payload bindings in match arms aren't propagating into the
classifier's solver). Worth chasing in Round 12.

## Correctness

- All 444 cargo tests pass with and without `EVIDENT_FUNCTIONIZE=1`.
- All 119 conformance tests pass in both modes.
- All cross-example demos produce identical stdout in both modes.

## Why this works after 10 rounds of false dawn

Rounds 2-10 built a function-izer that was structurally sound but
walled off from the actual runtime workload:
- The scheduler-side hook (Round 6) only fired when `pins` was
  empty — state-pair FSMs always passed pins.
- The classifier (Round 1) refused any Seq/Set var as unsupported.
- The native evaluator (Round 2-4) didn't handle `Expr::Call`,
  `Expr::SeqLit`, or `Expr::Index` — exactly the AST shapes that
  every realistic FSM body produces.

Round 11's three changes cleared all three blockers simultaneously
because they're entangled: a real FSM's body needs all three to
succeed, and any one missing causes a rejection.

## What this unblocks

- ~6 demos now benefit from per-tick µs eval after the first 1-2
  ticks.
- The infrastructure can be measured: future rounds widening the
  gate produce measurable per-demo speedups, not just synthetic-
  bench numbers.

## Round 12 candidates

1. **Pre-classify at load time** (~1 day). The first-tick
   classification + extraction is ~1ms on small claims; for a
   1-tick demo this WIPES the speedup. Hoist it to schema load.
   Cache key still uses `(name, sorted given_keys)` but check
   would happen once per claim shape.
2. **Investigate counter rejection** (~1 day). Multi-component
   classification is conservative — joint uniqueness should be a
   product of independent ones. Worth a focused debug session.
3. **Wider gate coverage** — same Mario-blocker list as Round 8/9
   (∀-over-Seq, ⇒-Implies, FFI types). With Round 11 done, these
   would land as measured wins, not just gate-acceptance numbers.

Recommend Round 12 = #1, biggest near-term shipping win.
