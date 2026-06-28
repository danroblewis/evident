"""Golden standard: (random_walk, fixedpoint_map).

EXPERT MATHEMATICS. A free 2-D king-move random walk has NO EQUILIBRIA:
  * No FIXED POINT. A fixed point is a state whose ONLY successor is itself. Every state here has 9
    successors (Δx, Δy ∈ {-1,0,+1} independently — verified: successors of any (a,b) are the full
    3×3 neighbourhood incl. the stay-put (a,b)), so no state is absorbing. solution_structure's own
    verdict is "nondeterministic" with fixed_points = None.
  * No LIMIT CYCLE / recurrent attractor. The walk is translation-invariant and unbounded; it never
    settles into a periodic orbit. There is no attracting set and no basin partition.

So the EXPERT CONTENT of this view for a random walk is the ABSENCE of equilibria, stated honestly —
fixed_point_count = 0, cycle_count = 0, has_equilibria = false. A view that drew a spurious fixed
point, or that blanked without saying "no equilibria", would be the failure.

(Honesty note the data also surfaces: `mode` should ideally be "all-conditions" — the global basin
partition — but for this infinite free walk the renderer falls back to "reachable" (from-init). That
fallback is recorded so the test can flag it; it does NOT change the no-equilibria conclusion.)
"""
import z3

from golden import Check, run_case
from region_oracle import _unrolled  # reused only for its z3 plumbing style; we probe directly below

SOURCE = "fsm random_walk\n    x, y ∈ Int := 0\n    -1 ≤ Δx ≤ 1\n    -1 ≤ Δy ≤ 1"


def _no_fixed_points(model, data):
    ok = data["fixed_point_count"] == 0
    return ok, f"fixed_point_count={data['fixed_point_count']} (a random walk has NO absorbing state)"


def _no_cycles(model, data):
    ok = data["cycle_count"] == 0
    return ok, f"cycle_count={data['cycle_count']} (no recurrent/periodic attractor for a free walk)"


def _no_equilibria_flag(model, data):
    ok = data["has_equilibria"] is False
    return ok, f"has_equilibria={data['has_equilibria']} (the view must state the ABSENCE honestly)"


def _sampled_some_states(model, data):
    """The view must actually sample the state space (so 'no equilibria' is a real finding, not a
    render that produced nothing). n_states > 0 distinguishes 'searched, found none' from 'blank'."""
    ok = data["n_states"] > 0
    return ok, f"n_states={data['n_states']} (must have searched a non-empty sample to conclude 'none')"


def _nine_successors_confirms_nonabsorbing(model, data):
    """Independent transition probe (NOT the renderer): a generic state has exactly 9 distinct
    successors incl. itself, so it is non-absorbing — corroborating fixed_point_count=0 from the math,
    not from the renderer's output."""
    base = z3.And(*model.assertions) if len(model.assertions) != 1 else model.assertions[0]
    ft = model.consts[model._first_tick_name]
    s = z3.Solver()
    s.add(base, ft == False, model.consts["_x"] == 3, model.consts["_y"] == 3)  # noqa: E712
    succ = set()
    while s.check() == z3.sat and len(succ) < 20:
        m = s.model()
        X, Y = m.eval(model.consts["x"]).as_long(), m.eval(model.consts["y"]).as_long()
        succ.add((X, Y))
        s.add(z3.Not(z3.And(model.consts["x"] == X, model.consts["y"] == Y)))
    ok = len(succ) == 9 and (3, 3) in succ
    return ok, f"successors of (3,3) = {len(succ)} (expect 9 incl. stay-put → non-absorbing)"


def _mode_is_all_conditions(model, data):
    """ASPIRATIONAL (expected to FAIL today): the equilibrium view should seed from ALL initial
    conditions (the global basin partition), not the single from-init reachable run. For this
    infinite free walk the renderer falls back to 'reachable'. This check documents that gap — a
    FAIL here is the regression signal, not a bug in the test."""
    ok = data["mode"] == "all-conditions"
    return ok, f"mode={data['mode']!r} (expected 'all-conditions'; 'reachable' = from-init fallback gap)"


CHECKS = [
    Check("no fixed points (no absorbing state)", _no_fixed_points),
    Check("no limit cycles (no recurrent attractor)", _no_cycles),
    Check("absence of equilibria stated honestly", _no_equilibria_flag),
    Check("searched a non-empty state sample", _sampled_some_states),
    Check("9 successors confirm non-absorbing (transition probe)", _nine_successors_confirms_nonabsorbing),
    Check("seeded from ALL initial conditions [aspirational]", _mode_is_all_conditions),
]


def case():
    return run_case("random_walk", SOURCE, "fixedpoint_map", CHECKS)
