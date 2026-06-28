"""Golden standard: (random_walk, value_heatmap).

The model: fsm random_walk; x, y ∈ Int := 0; -1 ≤ Δx ≤ 1; -1 ≤ Δy ≤ 1 — a 2-D king-move walk.

WHAT THE VIEW CLAIMS (its own docstring): "every carried variable's value over time, as a raster …
the whole trajectory into a single dense image". It walks up to MAX_TICKS (60) ticks from the initial
state and rasters one row per carried leaf, one column per tick, colored by value.

EXPERT EXPECTATION for a 2-D random walk (derived from the math, NOT from current output):
  * BOTH x and y are rows (the walk has two wandering coordinates).
  * Each row WANDERS — its value changes over ticks (n_distinct ≥ 2). A flat constant band for a
    coordinate that is free to step ±1 every tick is a regression (the raster shows nothing).
  * The trajectory is SUBSTANTIAL, not a near-instant halt. A free random walk NEVER settles on a
    fixed point — it can wander for the full 60-tick cap. So `ticks` should be ≈ `max_ticks`, and the
    walk should NOT report `halted`. (The renderer dedups visited states and calls a REVISIT a "fixed
    point"; a walk that returns to the origin then dies after a few ticks is exactly that bug.)
  * Each row's value RANGE is roughly SYMMETRIC about 0 (it wanders both directions from the origin).

CURRENT STATUS (observed, reported honestly — these are the FAILs that expose the regression):
  * The sampled walk steps (0,0) → (1,-1) → (0,0), REVISITS the origin, and the dedup'd walker flags
    it as a fixed point and HALTS at tick 3 of 60. So:
      - "trajectory is substantial (ticks ≈ max_ticks, not halted)"  → FAILS (3 ≪ 60, halted=True).
      - "each row wanders" → marginally passes (2 distinct values) but the raster is 3 columns wide —
        a dead picture, not the wandering band the view promises.
    The root cause is the SINGLE-sampled-dedup'd walk in render_value_heatmap._walk_with_flags +
    time_series_walk.walk: a nondeterministic walk should not be summarized as one revisit-halting
    chain. The fix is a renderer concern (ensemble or non-halting sampling for nondeterministic FSMs).
"""
from golden import Check, run_case

SOURCE = "fsm random_walk\n    x, y ∈ Int := 0\n    -1 ≤ Δx ≤ 1\n    -1 ≤ Δy ≤ 1"


def _both_axes_are_rows(model, data):
    rows = data["rows"]
    ok = "x" in rows and "y" in rows
    return ok, f"rows={rows} (expected both x and y — a 2-D walk has two wandering coordinates)"


def _each_row_wanders(model, data):
    """Every coordinate is free to step ±1 each tick, so over the trajectory its value must CHANGE —
    a flat (n_distinct==1) band means the raster shows a constant where the math says it wanders."""
    flat = {r: s["n_distinct"] for r, s in data["series"].items() if s["n_distinct"] < 2}
    ok = not flat
    counts = ", ".join(f"{r}:{s['n_distinct']}" for r, s in data["series"].items())
    return ok, (f"per-row n_distinct = {{ {counts} }}"
                + (f"; FLAT rows (never change) = {list(flat)}" if flat else ""))


def _trajectory_substantial(model, data):
    """A FREE random walk never settles — it can wander the full tick cap. So the rastered trajectory
    should be CLOSE to max_ticks and must NOT be reported as halted. A 3-of-60 halt is the
    revisit-as-fixed-point regression in the dedup'd single-run walker."""
    ticks, cap, halted = data["ticks"], data["max_ticks"], data["halted"]
    ok = (ticks >= 0.5 * cap) and not halted
    return ok, (f"rastered {ticks} of {cap} ticks, halted={halted} — a free walk should wander ≈the "
                f"full cap and NEVER halt on a fixed point (revisit-as-fixed-point = the regression)")


def _range_symmetric_about_origin(model, data):
    """A walk from the origin wanders both directions, so each coordinate's [min, max] should straddle
    0 (roughly symmetric). A one-sided range (e.g. min≥0) means the sampled run only ever stepped one
    way — an unrepresentative single run, not the symmetric spread the math predicts."""
    bad = {}
    for r, s in data["series"].items():
        lo, hi = s["min"], s["max"]
        if lo is None or hi is None:
            bad[r] = "no numeric values"
        elif not (lo <= 0 <= hi):
            bad[r] = f"[{lo}, {hi}] does not straddle 0"
    ok = not bad
    return ok, (f"per-row range straddles 0: {not bad}"
                + (f"; one-sided rows = {bad}" if bad else ""))


CHECKS = [
    Check("both x and y are rows", _both_axes_are_rows),
    Check("each coordinate row wanders (not a flat band)", _each_row_wanders),
    Check("trajectory is substantial (ticks ≈ cap, not halted)", _trajectory_substantial),
    Check("each row's value range straddles the origin (symmetric wander)", _range_symmetric_about_origin),
]


def case():
    # value_heatmap takes NO x_var/y_var — run_case falls back to the 3-arg IDE call automatically.
    return run_case("random_walk", SOURCE, "value_heatmap", CHECKS)
