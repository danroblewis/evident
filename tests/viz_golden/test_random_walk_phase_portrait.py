"""Golden standard: (random_walk, phase_portrait).

EXPERT MATHEMATICS (PATTERN.md; the shape is VERIFIED against region_oracle, an independent
transition unrolling — never read off the renderer):

  * random_walk is a 2-D NONDETERMINISTIC king-move walk: from any state there are 9 successors
    (Δx, Δy each ∈ {-1,0,+1}). The set of states it can occupy after k ticks is the CHEBYSHEV /
    L∞ SQUARE max(|x|,|y|) ≤ k — symmetric about the origin (its initial state), filling toward
    the diagonal corners (±k,±k), and growing with k.
  * A phase portrait should therefore show a 2-D region of motion FILLING the (x,y) plane around
    the origin — NOT a single fixed point, NOT a constant axis, and NOT one drifting trajectory.
  * A NONDETERMINISTIC walk has no single direction vector per state (9 successors, not one), so a
    deterministic single-arrow vector field would be misleading — the honest picture is the
    reachable cloud / the relational transition fan.

WHAT THE RENDERER DOES TODAY (the regression this golden test EXPOSES):
  * plan_channels picks (x,y) as numeric axes, but _numeric_regime classifies the model
    "degenerate" because it judges the FROM-INIT dwell (one seeded successor chain), which the
    helper collapses to a single point — so the renderer draws an N/A card:
    "reachable set is a single fixed point / a constant axis". That is WRONG for a walk that fills
    a square. Root cause = seed-from-initial_state() (the known "shows one run, not all initial
    conditions" regression).
  * Even m.reachable() (the renderer's own .data.json `reachable` field) is a CAPPED, seed-biased
    BFS that drifts asymmetrically — so it is NOT the symmetric square either. The TRUE reachable
    set (the golden reference) comes from region_oracle, not from the renderer.
"""
from golden import Check, run_case
from region_oracle import reachable_set

SOURCE = "fsm random_walk\n    x, y ∈ Int := 0\n    -1 ≤ Δx ≤ 1\n    -1 ≤ Δy ≤ 1"


def _axes_pinned(model, data):
    ax = data["axes"]
    ok = ax["x"] == "x" and ax["y"] == "y"
    return ok, f"plotted axes = {ax} (the test pinned x_var=x, y_var=y)"


def _true_region_is_symmetric_square(model, data):
    """SANITY on the math (independent of the renderer): the TRUE reachable set at k=3 is the
    symmetric, origin-centred, corner-filling L∞ square. If THIS fails the model/oracle is wrong;
    it anchors every renderer expectation below."""
    K = 3
    reached = reachable_set(model, "x", "y", K)
    square = {(X, Y) for X in range(-K, K + 1) for Y in range(-K, K + 1)}
    fills = square <= reached
    nothing_outside = not any(max(abs(X), abs(Y)) > K for (X, Y) in reached)
    symm = all((-X, Y) in reached and (X, -Y) in reached for (X, Y) in reached)
    corners = all(c in reached for c in ((K, K), (-K, K), (K, -K), (-K, -K)))
    ok = fills and nothing_outside and symm and corners
    return ok, (f"TRUE reachable set k={K}: fills L∞ square={fills}, nothing outside={nothing_outside}, "
                f"origin-symmetric={symm}, corners reachable={corners}")


def _not_rendered_na(model, data):
    """A phase portrait of a 2-D walk that fills a square must NOT be an N/A / placeholder card.
    EXPECTED TO FAIL TODAY: the renderer prints 'reachable set is a single fixed point' because it
    judges the seeded dwell, not the reachable fan."""
    na = data.get("rendered_na")
    return (not na), (f"rendered_na={na}, regime={data.get('regime')!r} — expected a real 2-D field/"
                      "cloud, not an N/A card (a free 2-D walk is NOT a fixed point)")


def _rendered_cloud_is_2d_symmetric(model, data):
    """The cloud the picture actually shows should span 2-D and be symmetric about the origin.
    EXPECTED TO FAIL: the from-init dwell is a single point (degenerate) or one drifting run."""
    r = data.get("rendered", {})
    sx, sy = r.get("x"), r.get("y")
    two_d = bool(sx and sy and sx[1] > sx[0] and sy[1] > sy[0])
    symm = r.get("symmetric_x") and r.get("symmetric_y")
    ok = two_d and symm
    return ok, (f"rendered cloud n={r.get('n')} x={sx} y={sy} symmetric_x={r.get('symmetric_x')} "
                f"symmetric_y={r.get('symmetric_y')} — expected a 2-D origin-symmetric region")


def _reachable_field_fills_symmetric_square(model, data):
    """The relational reachable cloud the renderer records should be the symmetric, corner-filling
    square. EXPECTED TO FAIL: m.reachable() is a capped seed-biased BFS that drifts one-sided, so
    it is neither symmetric nor a clean square (the deeper seed-bias regression)."""
    rr = data.get("reachable")
    if rr is None:
        return False, "no reachable cloud recorded"
    ok = rr.get("symmetric_x") and rr.get("symmetric_y") and rr.get("fills_corners")
    return ok, (f"reachable cloud n={rr.get('n')} x={rr.get('x')} y={rr.get('y')} "
                f"symmetric_x={rr.get('symmetric_x')} symmetric_y={rr.get('symmetric_y')} "
                f"fills_corners={rr.get('fills_corners')} — expected a symmetric square (seed bias breaks it)")


CHECKS = [
    Check("axes pinned to x,y (explicit x_var/y_var honored)", _axes_pinned),
    Check("TRUE reachable set is the symmetric L∞ square (math sanity)", _true_region_is_symmetric_square),
    Check("phase portrait is NOT an N/A card for a 2-D walk", _not_rendered_na),
    Check("rendered cloud is 2-D and origin-symmetric", _rendered_cloud_is_2d_symmetric),
    Check("recorded reachable cloud fills the symmetric square", _reachable_field_fills_symmetric_square),
]


def case():
    return run_case("random_walk", SOURCE, "phase_portrait", CHECKS, x_var="x", y_var="y")
