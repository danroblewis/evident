# Tactic-chain bench — 2026-05-16

Z3 ships many preprocessing tactics (`simplify`, `propagate-values`,
`solve-eqs`, `elim-uncnstr`, `der`, `ctx-solver-simplify`, etc.).
Each can be wired in front of the SMT solving step as a "free"
optimization. This bench measures which tactic chain gives the best
speedup on our actual workloads.

Reproduce:
```
cd runtime
cargo run --release --example bench_tactics -- 30 ../examples/test_22_prev_record.ev
cargo run --release --example bench_tactics -- 20 ../examples/test_09_two_fsms.ev
cargo run --release --example bench_tactics -- 50 ../examples/test_19_prev_tick.ev
cargo run --release --example bench_tactics -- 10 ../examples/test_18_reflection.ev
```

The bench loads the program, runs the FSM scheduler N ticks under
each `EVIDENT_TACTICS` setting (3 trials + warm-up each), reports
median wall time. Crucially, it also tracks the FSM's step count
per trial — tactics that produce wrong UNSAT short-circuit at step 0
and are flagged BROKEN, so their bogus "fast" timings don't pollute
the comparison.

## Results

| Chain | prev_record | two_fsms | prev_tick | reflection | mean |
|---|---|---|---|---|---|
| `off` (baseline)   |   1.00× |   1.00× |   1.00× |   1.00× |   1.00× |
| `simplify`         |   0.93× |   1.22× |   1.41× |   1.15× |   1.18× |
| `propagate-values` |   1.27× |   1.33× |   1.54× |   1.23× |   1.34× |
| **`solve-eqs`**    | **1.35×** | **1.54×** | **1.57×** | **1.30×** | **1.44×** |
| `der`              |   0.91× |   1.28× |   1.41× |   1.27× |   1.22× |
| `standard`         |   1.27× |   1.49× |   1.38× |   1.25× |   1.35× |
| `aggressive`       |   1.21× |   1.57× |   1.41× |   1.26× |   1.36× |
| `simp,solve-eqs`   |   1.27× |   1.51× |   1.50× |   1.18× |   1.37× |
| `simp,ctx-simp`    |   0.37× |   0.41× |   0.48× |   0.59× |   0.46× |
| `simp,elim-uncnstr`|   0.83× |   1.23× |   1.31× |   1.10× |   1.12× |
| `simp,der`         |   0.89× |   1.22× |   1.42× |   1.03× |   1.14× |
| `full,elim-pred`   |   1.31× | **2.28×** |   1.28× |   1.13× |   1.50× |
| `simp,prop,der`    |   1.34× |   1.24× |   1.21× |   1.07× |   1.21× |

Standard chain key: `standard = simplify,propagate-values,solve-eqs`;
`aggressive = standard + elim-uncnstr,propagate-ineqs`;
`full,elim-pred = aggressive + elim-predicates`.

Append `smt` (terminal solving tactic) is automatic — `simplify` and
similar preprocessors alone return `Unknown` without it. That's
wired into `make_tuned_solver`.

## Decision

**Default: `EVIDENT_TACTICS=solve-eqs`.**

Reasoning:
- Consistently in the top 3 on every workload.
- Highest mean speedup (1.44×) of any single tactic.
- Never regresses (no row below 1.30×).
- Simple to reason about: substitute equality-defined variables, then
  solve. No cascade of side-effects.
- The longer chains have higher peak speedups (`full,elim-pred` at
  2.28× on two_fsms) but variable — some are slower than baseline
  on other workloads.
- `ctx-solver-simplify` is always slower (0.37-0.59×) — uses the
  solver itself for simplification; not worth the overhead.

This default applies to every `rt.query` call. Users can override
via `EVIDENT_TACTICS=...`; common overrides:
- `EVIDENT_TACTICS=off` — baseline / debugging.
- `EVIDENT_TACTICS=standard` — slightly more aggressive normalization.
- `EVIDENT_TACTICS=simplify,propagate-values,solve-eqs,elim-predicates`
  — for claims with quantified function definitions.

## Correctness gates

The bench measures BROKEN vs working chains. Tactics that produce
spurious UNSAT on satisfiable problems are flagged and excluded.
With `smt` appended terminally, none of the chains in this matrix
produce spurious UNSAT — they all match baseline step counts. (An
earlier version without the `smt` append had `simplify`-alone
returning `Unknown`; that's fixed.)

The full `./test.sh` suite (12 lints + 422 cargo tests + 119
conformance) passes with `solve-eqs` as default.
