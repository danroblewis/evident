"""Golden standard: (random_walk, scatter_matrix).

EXPERT MATHEMATICS (PATTERN.md; shape VERIFIED against region_oracle):

  * The (x,y) cell of the scatter matrix is the position cloud of the 2-D king-move walk. The set
    of occupiable states is the symmetric, origin-centred, corner-filling L∞ SQUARE max(|x|,|y|)≤k.
    So the scatter cell should be a 2-D cloud SYMMETRIC about the origin, spreading equally in +/-x
    and +/-y, and reaching the diagonal corners (the L∞ square, NOT an L1 axis-only diamond).

WHAT THE RENDERER DOES TODAY (the regression this exposes):
  * scatter_matrix samples its cloud from sample_states(m) = a long FROM-INIT trajectory — ONE
    drifting random run. So one axis wanders far one-sided (e.g. y ∈ [-262, 1]) while the other
    barely moves: the cloud is NOT symmetric, NOT origin-centred, and does not fill the square.
    It shows ONE run, not the reachable set (the known seed-from-initial_state regression).
  * The .data.json also records the relational reachable fan (m.reachable()), but that BFS is
    capped + seed-biased too, so it is likewise asymmetric. The TRUE symmetric square is the
    region_oracle reference, asserted independently below.

scatter_matrix.render is (smt2, schema, out_path) — no axis kwargs — so axes are auto-picked
([y,x] by rank); the checks below are axis-order agnostic (they test BOTH plotted axes).
"""
from golden import Check, run_case
from region_oracle import reachable_set

SOURCE = "fsm random_walk\n    x, y ∈ Int := 0\n    -1 ≤ Δx ≤ 1\n    -1 ≤ Δy ≤ 1"


def _has_xy_pair(model, data):
    pv = data.get("pairwise_vars", [])
    ok = set(pv) >= {"x", "y"}
    return ok, f"pairwise_vars={pv} (the matrix must include the x,y projection cell)"


def _true_region_is_symmetric_square(model, data):
    K = 3
    reached = reachable_set(model, "x", "y", K)
    square = {(X, Y) for X in range(-K, K + 1) for Y in range(-K, K + 1)}
    fills = square <= reached
    symm = all((-X, Y) in reached and (X, -Y) in reached for (X, Y) in reached)
    corners = all(c in reached for c in ((K, K), (-K, K), (K, -K), (-K, -K)))
    ok = fills and symm and corners
    return ok, f"TRUE reachable k={K}: fills square={fills}, origin-symmetric={symm}, corners={corners}"


def _rendered_cloud_2d(model, data):
    """Both plotted axes must vary — a real 2-D cloud, not a near-1-D drift along one axis."""
    r = data.get("rendered", {})
    sx, sy = r.get("x"), r.get("y")
    ok = bool(sx and sy and sx[1] > sx[0] and sy[1] > sy[0])
    return ok, f"rendered cloud n={r.get('n')} x={sx} y={sy} (both axes must span — a 2-D cloud)"


def _rendered_cloud_symmetric(model, data):
    """The plotted cloud should be symmetric about the origin on BOTH axes. EXPECTED TO FAIL: the
    from-init trajectory drifts one-sided (e.g. y ∈ [-262,1]) — not symmetric, not origin-centred."""
    r = data.get("rendered", {})
    ok = r.get("symmetric_x") and r.get("symmetric_y")
    return ok, (f"rendered symmetric_x={r.get('symmetric_x')} symmetric_y={r.get('symmetric_y')} "
                f"x={r.get('x')} y={r.get('y')} — a drifting single run is not origin-symmetric")


def _centered_on_origin(model, data):
    c = data.get("center")
    ok = c == {"x": 0, "y": 0}
    return ok, f"init/center = {c} (the walk starts at the origin)"


CHECKS = [
    Check("matrix includes the (x, y) projection cell", _has_xy_pair),
    Check("TRUE reachable set is the symmetric L∞ square (math sanity)", _true_region_is_symmetric_square),
    Check("rendered cloud is 2-D (both axes span)", _rendered_cloud_2d),
    Check("rendered cloud is origin-symmetric (not one drifting run)", _rendered_cloud_symmetric),
    Check("cloud is centered on the origin", _centered_on_origin),
]


def case():
    return run_case("random_walk", SOURCE, "scatter_matrix", CHECKS)
