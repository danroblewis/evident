# Viz golden-test pattern — the per-view recipe (random_walk fan-out)

A golden test encodes what a DOMAIN EXPERT expects a diagram to show for a model, derived from the
model's TRUE MATHEMATICS (verified against an independent oracle where shape matters) — NEVER from
the current renderer output. The suite is NON-BLOCKING: a FAIL is the signal that a diagram regressed
or never met the standard. It is NOT wired into ./test.sh.

## The example
    fsm random_walk
        x, y ∈ Int := 0
        -1 ≤ Δx ≤ 1
        -1 ≤ Δy ≤ 1
A 2-D nondeterministic KING-MOVE walk: x and y step independently AND simultaneously each tick, each
Δ ∈ {-1,0,+1} (9 successors per state, incl. stay-put). KEY MATH (verified with region_oracle):
  * Reachable set after k ticks = the CHEBYSHEV / L∞ SQUARE max(|x|,|y|) ≤ k — the FILLED square
    [-k,k]², NOT the L1 taxicab diamond (the diamond is the 4-neighbour walk; this is 8-neighbour).
  * Occupancy after k ticks: a 2-D random walk's position is a sum of k iid steps → by the CLT it is
    asymptotically GAUSSIAN, peaked at the origin, with VARIANCE GROWING LINEARLY in k (σ² ∝ k, so
    spread σ ∝ √k). Symmetric about the origin under the 4 dihedral symmetries (x↔-x, y↔-y, x↔y).
  * NO fixed points, NO terminal/recurrent rest set: every state reaches every other; the walk never
    settles. Views that look for equilibria (fixedpoint_map, morse_graph, terminal_map, nullcline)
    should HONESTLY report "none" — that absence IS the correct expert content for those views.
  * UNBOUNDED in time: as k→∞ the set grows without bound. Finite-horizon shape + symmetry is what we pin.

## THE DUAL CONTRACT (must hold for every renderer touched — this is how the PRODUCT calls it)
The IDE (`ide/web/render.py`) calls EVERY renderer through one of these shapes and adapts by signature:
  * `render(smt2, schema, out_path, x_var=None, y_var=None)` — file paths; loads its own model.
  * `render(smt2, schema, out_path)` — file paths, no axes.
  * `render(model, out_path)` — takes a loaded Model (adapter `_render_via_model` passes one).
  * CLI `main()` (argv).
RULE: KEEP whatever signature a renderer already has. NEVER change a `(smt2,schema,out,...)` renderer
to take a Model, and NEVER change a `(model,out)` renderer to take paths — either breaks the IDE
adapter. Just ADD the `.data.json` emission inside the existing render body.
  * A paths renderer loads the model internally (it already does) — emit data there.
  * A `(model, out_path)` renderer already HAS the model — emit data directly.
VERIFY BOTH after each renderer:
  (a) IDE path: `ide.web.render._render_png("<view>", prefix[, x_var=, y_var=])` → real PNG bytes +
      `<out>.data.json` exists (mirror the unpack: it returns `(png, points)` — a 2-tuple).
  (b) Golden suite: `python3 tests/viz_golden/run.py` still runs.

## The `.data.json` substrate
Beside the PNG write `<out>.data.json`: a JSON dict of the diagram's ABSTRACT MEANING (not pixels),
built from the SAME analysis the renderer draws (so data can never disagree with the picture). Mirror
`viz/region_data.py` (`build` + `write`; `write` NEVER raises — a sidecar failure must not fail render).
Always include: `{"view", "model", ...the abstract content...}`. Examples of content per view:
  * occupancy_heatmap: the grid (counts/density), its peak cell, the axes, bounds, total samples.
  * reachability_tree: node count, per-node branching factor (should be ≤9), depth, root state.
  * time_series: per-tick ensemble spread (e.g. min/max or variance of each var across runs vs tick).
  * state_graph / transition_matrix: out-degree per node (≤9), node count, translation-invariance.
  * phase_portrait / scatter_matrix: the plotted (x,y) sample cloud + its bounds + axes.
Reuse `render_common.short`, `axis_select.write_axes` (axes echo), and for shape-truth checks the
independent `tests/viz_golden/region_oracle.py` (unrolls the transition; NOT the renderer).

## The test (tests/viz_golden/test_random_walk_<view>.py)
Expose `case()` returning `run_case("random_walk", SOURCE, "<view>", CHECKS, x_var=, y_var=)`.
Each CHECK is `Check(name, fn(model, data)->(ok, detail))` asserting an expert expectation derived
from the math above. Pin axes (x_var="x", y_var="y") for axis-taking views and assert they're echoed.
The harness `tests/viz_golden/golden.py::run_case` drives the renderer via the IDE contract and loads
the model SEPARATELY for oracle checks (never hands the renderer a Model). Note honestly PASS vs FAIL.

## Files you own per view
`viz/render_<view>.py` (+ a `viz/<view>_data.py` builder if it's more than a few lines),
`tests/viz_golden/test_random_walk_<view>.py`. DO NOT touch ide/web/static/*, run_router.py,
solve_router.py, model_query.py, server.py, figure_router.py.
