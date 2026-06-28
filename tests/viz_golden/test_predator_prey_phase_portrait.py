"""Golden standard: (predator_prey, phase_portrait).

EXPERT MATHEMATICS (Lotka-Volterra; derived from the model, NOT read off the renderer —
see tests/viz_golden/analysis/predator_prey.md):

  fsm predator_prey
      Δprey = _prey*0.1 - _prey*_pred*0.01      (dx/dt = x(α - β y),  α=0.1, β=0.01)
      Δpred = _prey*_pred*0.005 - _pred*0.1     (dy/dt = y(δ x - γ),  γ=0.1, δ=0.005)

  * Coexistence fixed point (γ/δ, α/β) = (20, 10) — a NEUTRAL CENTER (purely imaginary
    eigenvalues ±i√(αγ)). NOT the initial condition (40, 9).
  * Conserved quantity V = δx - γ·ln x + βy - α·ln y is constant on each orbit ⇒ the
    solution space is a FAMILY OF NESTED CLOSED LOOPS encircling (20, 10).
  * A phase portrait must therefore show the SOLUTION SPACE (many initial conditions ⇒
    nested loops around (20,10)), NOT a single from-init trajectory.
  * The forward-Euler (h=1) discretization does NOT conserve V — it spirals outward and
    diverges. That is an INTEGRATOR ARTIFACT, not the system; a diagram that reports
    "unbounded" or traces one diverging run is WRONG about the mathematics.

WHAT THE RENDERER DOES TODAY (the regressions this golden test EXPOSES):
  * `center` in the data is the INITIAL CONDITION (40, 9), not the true fixed point (20, 10).
  * `rendered` is the n=201 dwell from the single seed (40, 9) — ONE run, not the orbit
    family. The substrate exposes no nested-loop solution space.
  * `reachable` carries 1e18 extents (Euler divergence) — the relational reachable set
    blows up because the integrator injects energy every step.
"""
import math

from golden import Check, run_case

SOURCE = ("fsm predator_prey\n"
          "    prey ∈ Real := 40.0\n"
          "    pred ∈ Real := 9.0\n"
          "    Δprey = _prey * 0.1 - _prey * _pred * 0.01\n"
          "    Δpred = _prey * _pred * 0.005 - _pred * 0.1")

ALPHA, BETA, GAMMA, DELTA = 0.1, 0.01, 0.1, 0.005
FP = (GAMMA / DELTA, ALPHA / BETA)            # (20, 10) — the true coexistence center


def _V(x, y):
    return DELTA * x - GAMMA * math.log(x) + BETA * y - ALPHA * math.log(y)


def _fixed_point_is_center_not_init(model, data):
    """MATH SANITY (independent of renderer): the coexistence fixed point is (20,10), and the
    forward-Euler map spirals OUTWARD (V increases) from the seed (40,9). Anchors every
    renderer expectation below."""
    x, y = 40.0, 9.0
    V0 = _V(x, y)
    for _ in range(120):
        x, y = x + x * 0.1 - x * y * 0.01, y + x * y * 0.005 - y * 0.1
    drift = _V(x, y) - V0
    ok = abs(FP[0] - 20.0) < 1e-9 and abs(FP[1] - 10.0) < 1e-9 and drift > 0
    return ok, (f"true fixed point (γ/δ,α/β)={FP} (NOT the init (40,9)); "
                f"Euler ΔV over 120 ticks = {drift:+.4f} (>0 ⇒ outward spiral / integrator artifact)")


def _center_is_fixed_point_not_init(model, data):
    """The phase portrait's marked `center` must be the equilibrium (20,10), not the seed (40,9).
    EXPECTED TO FAIL TODAY: center == initial_state."""
    c = data.get("center") or {}
    cx, cy = c.get("x"), c.get("y")
    ok = cx is not None and abs(cx - FP[0]) < 1.0 and abs(cy - FP[1]) < 1.0
    return ok, (f"center in data = ({cx}, {cy}); expected the fixed point {FP}, "
                f"not the initial condition (40, 9)")


def _shows_solution_space_not_one_run(model, data):
    """The phase portrait must depict the SOLUTION SPACE — a family of nested closed orbits
    around (20,10) from MANY initial conditions — not a single from-init trajectory.
    EXPECTED TO FAIL TODAY: `rendered` is the n=201 dwell of the one seeded orbit; there is no
    orbit-family substrate."""
    r = data.get("rendered", {})
    # A single-orbit dwell has exactly one loop's worth of points and is NOT origin/center-
    # symmetric in a way that reflects a sampled family. We require an explicit multi-orbit
    # signal in the substrate (an orbit family / multiple seeds), which today is absent.
    fam = data.get("orbits") or data.get("orbit_family") or data.get("seeds")
    n_fam = len(fam) if isinstance(fam, (list, tuple)) else (fam or 0)
    ok = bool(n_fam and n_fam >= 2)
    return ok, (f"orbit-family size in data = {n_fam} (rendered.n={r.get('n')} is a single "
                f"from-init dwell); expected ≥2 sampled orbits forming nested closed loops")


def _reachable_not_polluted_by_euler_divergence(model, data):
    """The recorded reachable cloud should reflect BOUNDED closed orbits, not the Euler blow-up.
    EXPECTED TO FAIL TODAY: `reachable` carries ~1e18 extents (forward-Euler injects energy)."""
    rr = data.get("reachable") or {}
    xs, ys = rr.get("x") or [0, 0], rr.get("y") or [0, 0]
    huge = max(abs(xs[0]), abs(xs[1]), abs(ys[0]), abs(ys[1]))
    ok = huge < 1e6
    return ok, (f"reachable extent magnitude = {huge:.3g} (x={xs} y={ys}); expected a bounded "
                f"closed-orbit region, not the forward-Euler divergence (≈1e18)")


CHECKS = [
    Check("fixed point is the center (20,10); Euler spirals outward (math sanity)",
          _fixed_point_is_center_not_init),
    Check("marked center is the fixed point (20,10), not the initial condition",
          _center_is_fixed_point_not_init),
    Check("phase portrait shows the SOLUTION SPACE (≥2 nested orbits), not one run",
          _shows_solution_space_not_one_run),
    Check("recorded reachable cloud is bounded (not the Euler 1e18 divergence)",
          _reachable_not_polluted_by_euler_divergence),
]


def case():
    return run_case("predator_prey", SOURCE, "phase_portrait", CHECKS,
                    x_var="prey", y_var="pred")
