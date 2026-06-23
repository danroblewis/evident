# Functionizer visualization

The functionizer reduces a solved FSM transition relation to per-variable **functions** (the JIT's
update law) plus a **residual** of constraints it couldn't make into functions (the genuinely
relational part). That decomposition is interesting to *see* — it's the compiled structure of the
program, distinct from its dynamics. This is the design of the six IDE views that show it.

## The two data sources

There are two ways to get the decomposition, with the same shape:

1. **Python re-derivation** — `viz/functionize.py:extract_functions(model)` re-derives it from the
   model's already-loaded z3 assertions (`model.assertions`). It peels `Implies(guard, var=body)` →
   `Guarded`, `var = expr` (incl. the Δ-form `var − _var == rhs`) → `Scalar`, everything else →
   residual. Fast (no round-trip), and it's what the live views use today.

2. **Authoritative Rust export** — `evident functions <file>` (runtime
   `EvidentRuntime::export_functions`) dumps the *real* `Z3Program` the runtime compiled, as JSON.
   `viz/functionize_authoritative.py:load_authoritative` parses it back into the same shape. This is
   the source of truth (no drift) and is strictly more robust: the Python path loads the model via
   `z3.parse_smt2_file`, which **dies on effect-heavy FSMs** (the runtime's SMT-LIB datatype accessors
   trip z3's parser, "repeated accessor f0"); the authoritative dump never round-trips SMT-LIB.

The two now agree on var-sets (oracle-checked) because the runtime functionizer was taught the Δ-form
and the guarded-init `(or guard eq)` shape — see "Functionizer coverage" below.

## The six views (all opt-in tabs, never auto-recommended)

| View | Shows | Renderer |
|---|---|---|
| `function_graph` | the compiled data-flow DAG — edge W→V when V reads W's prev; a feedback cycle = coupled dynamics, acyclic = driven/autonomous | `viz/render_function_graph.py` |
| `function_residual` | computation vs constraint — the functions beside the standing-invariant residual; "% computed" = carried-with-update-law / total-carried | `viz/render_function_residual.py` |
| `function_guards` | the guard decision trees (atoms trie'd into the nested decision) **+ a z3 verdict**: total & unambiguous, or ⚠ INCOMPLETE / ⚠ OVERLAPPING **with the witness input** | `viz/render_function_guards.py` |
| `function_behavior` | each function's transfer map sampled over its inputs (enum → guard partition, numeric → surface); discrete maps draw unconnected points (no fabricated continuity) | `viz/render_function_behavior.py` |
| `function_complexity` | per-variable AST-size heuristic (branches + arithmetic) — relative compile weight, *not* measured from Cranelift | `viz/render_function_complexity.py` |

Supporting: `function_summary(model)` classifies coupling (coupled / driven / autonomous self-map) and
computes the honest "% computed"; `guard_analysis(model, steps, residual)` is the z3 totality/
disjointness check over the **declared type domain** (∩ residual invariants), returning the gap/overlap
witness. `function_diff(ma, mb)` powers the "⚙ compiled structure" section of the model-diff.

## Two correctness invariants worth keeping

- **Honest % computed.** The denominator is carried-vars-with-an-update-law / total-carried. A residual
  type-bound (`0 ≤ timer ≤ 2`) is a *standing invariant*, not un-computed work — it must never count
  against the percentage. (An earlier version read elevator as "50% computed" though both vars were
  functionized.)
- **Never fabricate structure.** `function_behavior` line-connects only continuous (Real, single-branch)
  maps; discrete/guarded/integer outputs draw unconnected points, so Collatz's even/odd sawtooth isn't
  smeared into a fake smooth curve. The guard verdict states it checks the declared type domain, not the
  reachable set (so a ⚠ may name an unreachable corner).

## Functionizer coverage (a perf note)

The authoritative export is also a precise tool for finding shapes the runtime *doesn't* JIT. The Δ-form
(`x − _x == step`, compound LHS) and guarded-init (`is_first_tick ⇒ x = e`, lowered to a disjunction)
were both staying residual — so typical FSMs ran the slow path. Teaching `extract_program_partial`
those two shapes (`solve_delta_side` + `try_classify_guarded`) made them JIT. The one remaining
under-functionized example is the ΔΔ oscillator, partly *inherently*: its `pos_milli` is floor-defined
by inequalities (`pos_milli ≤ pos·1000 < pos_milli+1`) — genuinely relational, correctly residual.
