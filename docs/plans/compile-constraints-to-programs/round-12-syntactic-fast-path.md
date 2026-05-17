# Round 12 — Syntactic fast path (skip Z3 classify)

**Outcome:** SHIPPABLE. Removes the Z3-based classifier from the
hot path. Tiny 1-tick demos go from "regression vs baseline" to
"~24× faster than baseline". Larger workloads inherit a further
~25% boost on top of Round 11's wins.

## What changed

### `try_extract_one_chain` — one big chain, no decomposition

Round 11 used `classify_components` to:
  1. Translate the body into Z3 assertions
  2. Decompose them into independent components
  3. For each component, run a 2-copy uniqueness check
  4. Per functional component, call `extract_chain_xl`

Steps 1-3 require Z3 and dominate the first-tick cost (~1-1.5 ms on
small claims). Step 4 is pure AST work.

The new fast path bypasses 1-3:
- The gate already vets that every body Constraint is a pure
  equality (no Forall/Exists/Implies/Ternary/Match-as-constraint).
- Under that constraint, a complete substitution chain (one
  defining equation per output var, topo-sortable) IS the
  functional witness — uniqueness is structural, not Z3-derived.

`try_extract_one_chain(schema, given_keys, ...)` collects every
non-given Membership into a single mega-component and runs
`extract_chain_xl`. If it returns Some, the body is functional by
construction. If it returns None (some output has no defining
equation, or there's a cycle), the slow path runs as before.

### Type-shape sanity check (closes a soundness hole)

The Z3 translator's "dropped constraint" fatal-exit catches
incompatibilities like `v ∈ IVec2 = 5` (composite LHS, scalar RHS).
The fast path doesn't translate, so it would silently accept the
body and produce wrong SAT results.

Mitigation: after extracting the chain, walk each step and check
that LHS declared type is compatible with RHS shape. The narrow
rule: if LHS's declared type is a non-primitive, non-enum,
non-Seq/Set user type (e.g. `IVec2`, `Color`), and the RHS is a
bare scalar literal (`Int`/`Real`/`Bool`/`Str`), refuse the chain
and fall through.

This is enough to keep `tests/vector_arith.rs::vec_scalar_broadcast_is_rejected`
green (the canonical "must reject" test for invalid programs).

## Measured speedups

`/tmp/bench_counter_1k.ev` — 1001-tick counter:

```
                    wall          solve     per-tick solve
Baseline:        288.59ms       282.94ms       0.283 ms
Round 11:          8.40ms         2.87ms       0.003 ms   (99.5x solve)
Round 12:          6.64ms         2.32ms       0.002 ms   (122x solve, 43x wall)
```

`/tmp/bench_counter.ev` — 200-tick counter:

```
                    wall          solve     per-tick solve
Baseline:         63.47ms        61.99ms       0.308 ms
Round 12:          2.51ms         1.59ms       0.008 ms   (39x solve, 25x wall)
```

`examples/test_01_hello.ev` — 1 tick (where Round 11 REGRESSED):

```
                    wall          solve     per-tick solve
Baseline:          0.74ms         0.71ms       0.71 ms
Round 11:          1.35ms         1.32ms       1.32 ms   ← 1.86x SLOWER
Round 12:          0.05ms         0.03ms       0.03 ms   ← 24x FASTER
```

The Round 11 regression on 1-tick demos was the blocker for
`EVIDENT_FUNCTIONIZE=1` as default. Round 12 removes that
regression — every measured workload is now ≥ as fast as baseline.

## Correctness

- All 444 cargo tests pass with and without `EVIDENT_FUNCTIONIZE=1`.
- All 119 conformance tests pass in both modes.
- All cross-example demos produce identical stdout in both modes.
- The type-shape sanity check preserves the dropped-constraint
  fatal-exit behavior the Z3 path used to provide.

## Why this works

Three independent invariants combine to make the fast path sound:

1. **Gate vets the body shape.** `gate_diagnostics` already
   refuses any non-equality Constraint (Forall/Exists/Implies/
   Ternary-as-constraint/etc.). So the body is, by construction,
   a system of equalities `lhs = rhs`.

2. **`extract_chain_xl` vets completeness.** It returns Some iff
   every output variable has exactly one defining equation AND
   the equations topo-sort without cycles. Failure → fall
   through to Z3.

3. **Type-shape check vets primitives-vs-composites.** Catches
   the narrow case where Z3 would have fatal-exited via
   "dropped constraint".

The remaining failure modes (eval-time lookup of an unpinned var,
Match with no covering arm) cause `evaluate_chain` to return None
at runtime, gracefully degrading to Z3.

## Round 13 candidates

1. **Wider gate coverage** — same Mario-blocker list as Rounds
   8/9 (∀-over-Seq, ⇒-Implies, FFI types). Each closed gate
   unlocks specific demos. With Round 12 done, the per-tick cost
   floor is 30 µs — gate widening yields direct measurable wins.
2. **Pre-compile chains at load time** — the first cache miss
   per (schema, given-keys) still pays the AST extraction cost.
   For very-short-lived programs, hoisting this to load would
   save another 30-50 µs.
3. **Component-level partial compilation** (still the long-term
   Mario path). With the fast-path infrastructure in place,
   per-component function-izing is a natural next step.

Recommend Round 13 = #1, gate widening. The infrastructure is now
fast enough that each new gate-opening produces direct demo wins
instead of just synthetic-bench improvements.
