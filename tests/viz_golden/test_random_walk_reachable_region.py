"""Golden standard: (random_walk, reachable_region).

The model (the 2-D nondeterministic walk the brief names):

    fsm random_walk
        x, y ∈ Int := 0
        -1 ≤ Δx ≤ 1
        -1 ≤ Δy ≤ 1

EXPERT MATHEMATICS (derived, then VERIFIED against the transition with region_oracle — never read
off current output):

  * Δx and Δy are chosen INDEPENDENTLY and SIMULTANEOUSLY each tick, each in {-1, 0, +1}. So one tick
    can move diagonally — a step (+1, +1) costs ONE tick, not two. After k ticks the reachable set is
    therefore the CHEBYSHEV (L∞) BALL  max(|x|, |y|) ≤ k  — the filled SQUARE [-k, k]², NOT the L1
    "taxicab" diamond |x|+|y| ≤ k. (The diamond is the math for a 4-NEIGHBOUR walk that moves in
    exactly ONE axis per tick; this model is the 8-neighbour / king-move walk, so the square is exact.)
    The oracle confirms this at k = 1, 2, 3: every square point reachable, nothing outside reachable,
    and the corner (k, k) is attained at exactly tick k.
  * The region is SYMMETRIC about the origin (the seeded init x=y=0) and GROWS with the step bound.
  * As k → ∞ the set is UNBOUNDED — it grows without bound. So the renderer's free-walk verdict
    "unbounded" is HONEST; the abstract content the test pins is the finite-horizon SHAPE + symmetry.

CONSEQUENCE FOR THE RENDERER: because the true region is the L∞ SQUARE, a per-VARIABLE box
[lo_x, hi_x] × [lo_y, hi_y] is the EXACT reachable region at each horizon — TIGHT, not a loose
over-approximation. (Had the model been the L1 diamond, the box would be a 2×-too-large square and
THAT would be the golden failure.) This is the value of deriving the assertion from the math: it
tells us the box analysis is correct FOR THIS MODEL, and exactly when it would not be.
"""
from golden import Check, run_case
from region_oracle import reachable_set, reachable_at_exactly

SOURCE = "fsm random_walk\n    x, y ∈ Int := 0\n    -1 ≤ Δx ≤ 1\n    -1 ≤ Δy ≤ 1"


# ---- expert expectations over the renderer's .data.json (+ the transition oracle) ----

def _two_numeric_axes(model, data):
    nv = data["numeric_vars"]
    ok = set(nv) == {"x", "y"}
    return ok, f"numeric_vars={nv} (expected exactly x, y — a 2-D walk)"


def _axes_pinned(model, data):
    ax = data["axes"]
    ok = ax["x"] == "x" and ax["y"] == "y"
    return ok, f"plotted axes = {ax} (the test pinned x_var=x, y_var=y)"


def _centered_on_origin(model, data):
    c = data.get("center")
    ok = c == {"x": 0, "y": 0}
    return ok, f"init/center = {c} (expected the origin x=y=0)"


def _not_degenerate(model, data):
    """The region must not collapse to a point or a line — a 2-D walk fills 2-D area. (For the FREE
    walk the verdict is 'unbounded' and the box may be empty; this check only bites when a box exists,
    so it catches a renderer that drew a degenerate box for a BOUNDED variant.)"""
    box = data.get("box") or {}
    if not box:
        return True, "no finite box (free walk → unbounded); degeneracy check N/A"
    spans = {k: hi - lo for k, (lo, hi) in box.items()}
    ok = all(s > 0 for s in spans.values())
    return ok, f"box spans = {spans} (every axis must have positive extent — not a point/line)"


def _verdict_honest_unbounded(model, data):
    """The FREE walk grows without bound in time; 'unbounded' (or a bounded box that itself contains
    the origin) is honest. A verdict of 'unknown' for a clearly-numeric 2-D walk would be the failure."""
    v = data["verdict"]
    ok = v in ("unbounded", "bounded")
    return ok, f"verdict={v!r} (free walk grows without bound; must not be 'unknown'/'indeterminate')"


def _region_is_Linf_square_not_L1_diamond(model, data):
    """THE LOAD-BEARING expert check, probed directly on the transition (not on .data.json): at a
    small finite horizon the reachable set is the FILLED L∞ square, and is STRICTLY LARGER than the
    L1 diamond — i.e. the diagonal corners (±k, ±k) ARE reachable. Asserting the diamond here would
    be WRONG for this model; this check encodes that distinction."""
    K = 3
    reached = reachable_set(model, "x", "y", K)
    square = {(X, Y) for X in range(-K, K + 1) for Y in range(-K, K + 1)}     # max(|x|,|y|) ≤ K
    diamond = {(X, Y) for (X, Y) in square if abs(X) + abs(Y) <= K}
    fills_square = square <= reached
    nothing_outside = not any(max(abs(X), abs(Y)) > K for (X, Y) in reached)
    corners_reachable = all((c in reached) for c in ((K, K), (-K, K), (K, -K), (-K, -K)))
    strictly_bigger_than_diamond = (square - diamond) <= reached and corners_reachable
    ok = fills_square and nothing_outside and strictly_bigger_than_diamond
    missing = sorted(square - reached)
    return ok, (f"k={K}: fills L∞ square={fills_square}, nothing outside={nothing_outside}, "
                f"corners reachable (NOT an L1 diamond)={corners_reachable}; "
                f"square points missing={missing[:6]}")


def _frontier_grows_each_tick(model, data):
    """The region GROWS with the step bound: the corner (k, k) is reachable at EXACTLY tick k for
    several k (so each tick the extent expands by 1 in every direction — a growing region)."""
    grows = all(reachable_at_exactly(model, "x", "y", k, k, k) for k in (1, 2, 3))
    return grows, "corner (k,k) attained at exactly tick k for k=1,2,3 (extent grows each tick)"


CHECKS = [
    Check("two numeric axes (x, y)", _two_numeric_axes),
    Check("axes pinned to x,y (explicit x_var/y_var honored)", _axes_pinned),
    Check("region centered on the origin (init x=y=0)", _centered_on_origin),
    Check("region is 2-D (not a point or a line)", _not_degenerate),
    Check("verdict is honest for a free walk (not 'unknown')", _verdict_honest_unbounded),
    Check("reachable region is the L∞ SQUARE, not the L1 diamond", _region_is_Linf_square_not_L1_diamond),
    Check("region grows each tick (corner (k,k) at exactly tick k)", _frontier_grows_each_tick),
]


def case():
    """The golden case record — axes EXPLICITLY pinned (x_var='x', y_var='y') to demonstrate the
    override path; the renderer echoes them into .data.json['axes'], asserted by _axes_pinned."""
    return run_case("random_walk", SOURCE, "reachable_region", CHECKS, x_var="x", y_var="y")
