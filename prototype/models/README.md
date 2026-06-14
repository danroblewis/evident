# models — sub-model composition + runtime-owned unrolling (POC)

A **sub-model** is a named, parameterized constraint template (a custom predicate
or function). We compose sub-models by calling them (kwargs = the outside↦inside
variable mapping) and the runtime OWNS the recursion via an explicit bounded
unroller — Python lends a `for`-loop, never its call stack, so the mechanism
transfers to a real runtime.

## Pieces (`core.py`)

- `Model(name, params, body)` — a named predicate/function sub-model; `body`
  returns a Bool (predicate) or a term (function), minting internals with
  `fresh()` so a sub-model used twice doesn't alias its internals.
- `Transition(name, fields, step, uses)` — a one-step state transition;
  `step(cur, nxt) -> Bool` over field dicts.
- **Two execution strategies for the same transition:**
  - `run_oneshot` — build states `s0..s_fuel` in ONE solve. Variable count grows
    with depth.
  - `run_incremental` — solve one step at a time, reusing the SAME field slots
    (memory reuse; constant footprint = the tail-recursion runtime).
- `section_md` / `write_report` — emit a markdown file: **each sub-model
  prettified on its own**, the transition, and the **combined** unrolled model
  the runtime solves.

## Run

```bash
python3 -m models.examples      # → results/models.md (prettified Z3-AST report)
```

Run it from the `prototype/` directory (so `benchsuite` is importable). The
output path is absolute, so the file always lands in `prototype/results/`.

- `sum_to` — tail-recursive accumulator (sum 1..n).
- `list_max` — a transition that **composes** a value sub-model `at` (a fixed
  list as a lookup); iterative max. The report shows `at` by itself, the
  transition, and the full unrolled model.

The reports make the memory-reuse contrast concrete (e.g. list_max: 18 vars
one-shot vs 4 vars constant incremental) and show the per-step state trace.

## Scope

Handles **tail recursion** (iteration, fixed state). General/stack recursion
(true PDA) is deferred — it becomes "carry an explicit stack field." Fixed-point
detection/rewrite is a separate future layer (`docs/notes/fixed-point-models.md`).
