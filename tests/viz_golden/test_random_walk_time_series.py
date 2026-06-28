"""Golden standard: (random_walk, time_series).

The model (the 2-D nondeterministic king-move walk):

    fsm random_walk
        x, y ∈ Int := 0
        -1 ≤ Δx ≤ 1
        -1 ≤ Δy ≤ 1

EXPERT MATHEMATICS (derived, not read off current output):

  * A time series of a STOCHASTIC system is an ENSEMBLE of trajectories — the diagram's whole content
    is the SPREAD of reachable values at each tick, not one line. So a faithful random-walk time series
    must show MORE THAN ONE trajectory (n_trajectories > 1).
  * The position after t ticks is a sum of t i.i.d. steps Δ ∈ {-1,0,+1} (mean 0). By the standard
    random-walk result Var[x_t] = t · Var[Δ] and Var[y_t] = t · Var[Δ]: the VARIANCE GROWS LINEARLY in
    t, so the ensemble envelope WIDENS like √t — monotonically, from 0 at t=0. (Feller, *An Introduction
    to Probability Theory and Its Applications*, Vol. 1, ch. III — the simple random walk; and
    docs/design/state-space-diagrams.md, §3 "time series … a run's trajectory; … the proved envelope".)
  * BOTH x and y wander independently and symmetrically — neither stays pinned. The mean of each stays
    ~0; the band is symmetric about 0.

So the golden expectations are: an ensemble (>1 run); BOTH x and y have a NON-ZERO spread; and the
spread GROWS with tick (late ticks strictly wider than early ticks). These are the random-walk
diffusion signature.

CURRENT STATUS — a known REGRESSION the user reported, which these checks EXPOSE: random_walk's
carried vars are UNBOUNDED (no `≤` bound), so `ensemble_inits` returns None and the renderer falls
back to a SINGLE deterministic run (`mode == "single_run"`, n_trajectories == 1). Worse, the single
Z3-chosen successor chain is degenerate — x stays at 0 and y ramps monotonically — so EVERY tick's
spread is 0. The ensemble/spread checks therefore FAIL, which is the correct signal: the diagram does
not convey the random walk's diffusion. The fix (out of this test's scope) is to seed a real ensemble
for an unbounded nondeterministic walk instead of collapsing to one chain.
"""
from golden import Check, run_case

SOURCE = "fsm random_walk\n    x, y ∈ Int := 0\n    -1 ≤ Δx ≤ 1\n    -1 ≤ Δy ≤ 1"


def _both_axes_tracked(model, data):
    nv = set(data["numeric_vars"])
    ok = {"x", "y"} <= nv
    return ok, f"numeric_vars={sorted(nv)} (both x and y must be plotted)"


def _is_ensemble(model, data):
    """A stochastic walk's time series must show MORE THAN ONE trajectory — the spread IS the content.
    A single run (the unbounded-init fallback) cannot show diffusion. EXPECTED TO FAIL today."""
    n = data["n_trajectories"]
    ok = data["mode"] == "ensemble" and n > 1
    return ok, f"mode={data['mode']!r}, n_trajectories={n} (need an ensemble of >1 run; " \
               f"single_run/1 is the unbounded-init regression)"


def _nonzero_spread(model, data):
    """Each numeric var must have a NON-ZERO spread at some tick — a flat zero-spread track means the
    walk isn't wandering at all. EXPECTED TO FAIL today (single degenerate chain ⇒ all ranges 0)."""
    detail, allok = [], True
    for v in ("x", "y"):
        rng = [r for r in data["spread"].get(v, {}).get("range", []) if r is not None]
        mx = max(rng) if rng else 0
        allok = allok and mx > 0
        detail.append(f"{v} max range={mx}")
    return allok, "; ".join(detail) + " (each must exceed 0 — the ensemble must spread)"


def _spread_grows_with_tick(model, data):
    """The random-walk diffusion signature: the ensemble band WIDENS with t (Var ∝ t). Compare the
    spread early vs late — late must be strictly wider. EXPECTED TO FAIL today (all ranges 0)."""
    detail, allok = [], True
    for v in ("x", "y"):
        rng = [r for r in data["spread"].get(v, {}).get("range", []) if r is not None]
        if len(rng) < 4:
            allok = False
            detail.append(f"{v}: too few ticks ({len(rng)})")
            continue
        early = rng[len(rng) // 4]            # ~25% through
        late = rng[-1]                        # final tick
        grew = late > early
        allok = allok and grew
        detail.append(f"{v}: early={early} late={late} grew={grew}")
    return allok, "; ".join(detail) + " (band must widen ~√t — variance grows linearly)"


CHECKS = [
    Check("both x and y are tracked", _both_axes_tracked),
    Check("shows an ENSEMBLE (>1 trajectory), not a single run", _is_ensemble),
    Check("each var has non-zero spread (the walk wanders)", _nonzero_spread),
    Check("spread GROWS with tick (diffusion ~√t)", _spread_grows_with_tick),
]


def case():
    """time_series takes no projection axes — run_case falls back to the 3-arg IDE call."""
    return run_case("random_walk", SOURCE, "time_series", CHECKS)
