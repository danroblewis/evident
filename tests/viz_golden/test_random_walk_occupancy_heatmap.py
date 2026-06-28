"""Golden standard: (random_walk, occupancy_heatmap).

EXAMPLE (the 2-D nondeterministic king-move walk):
    fsm random_walk
        x, y ∈ Int := 0
        -1 ≤ Δx ≤ 1
        -1 ≤ Δy ≤ 1

EXPERT MATHEMATICS (the occupancy density a domain expert expects). A 2-D random walk's position
after k ticks is a sum of k iid steps; by the Central Limit Theorem the occupancy is asymptotically
GAUSSIAN — peaked at the ORIGIN, with VARIANCE GROWING LINEARLY in time (σ² ∝ k), and SYMMETRIC about
the origin under the walk's dihedral symmetries (x↔−x, y↔−y, x↔y; the step distribution on (Δx,Δy) is
invariant under all of them). The mean stays at the origin for all time. So the occupancy heatmap MUST:
  * peak at (≈0, ≈0) — the most-dwelt cell is the origin;
  * have its centroid (mean) at ≈(0, 0);
  * be ISOTROPIC — comparable spread in x and y (the two axes are statistically identical);
  * be SYMMETRIC about the origin — the density at (a,b) ≈ density at (−a,−b);
  * be genuinely 2-D — non-degenerate spread on BOTH axes (not a line).

Sources: BU Redner, *A Guide to First-Passage Processes* / "Random Walk/Diffusion" ch. 2
(http://physics.bu.edu/~redner/542/book/rw.pdf); Harvard Schwartz, "Lecture 2: Diffusion"
(https://scholar.harvard.edu/files/schwartz/files/2-diffusion.pdf) — MSD grows linearly in t, the
asymptotic distribution is Gaussian about the origin (CLT), independent of the single-step law.

CURRENT STATUS (the regression these checks EXPOSE): the renderer's `collect_numeric` builds the
density from `m.reachable()` — a BFS enumeration of distinct states of an UNBOUNDED walk — plus ONE
trajectory. That is NOT an ensemble occupancy: it sprays ~2000 distinct, mostly-singleton states
lopsidedly along whichever axis the BFS expands first. Observed data.json: mean ≈ (−1, −328), spread
≈ (0.85, 193) — grossly off-origin and anisotropic by ~200×, a near-1-D smear. So the symmetry, peak,
mean, and isotropy checks FAIL. The EXPECTED fix is an ENSEMBLE collector: sample many independent
fixed-length walks and 2-D-histogram their endpoints/visits (a real diffusion density).
"""
from golden import Check, run_case

SOURCE = "fsm random_walk\n    x, y ∈ Int := 0\n    -1 ≤ Δx ≤ 1\n    -1 ≤ Δy ≤ 1"

# Tolerances scaled to the grid: an honest sampled density won't be perfectly symmetric, but it must
# be near-origin and roughly isotropic. These are deliberately LOOSE so only a real regression fails.
_PEAK_TOL = 3.0          # the peak cell must sit within this of the origin on each axis
_MEAN_TOL = 3.0          # the centroid must sit within this of the origin on each axis
_ISO_RATIO = 4.0         # max/min axis spread ratio allowed (isotropy); a true RW is ~1


def _has_grid(data):
    return data.get("status") == "ok" and data.get("grid")


def _axes_pinned(model, data):
    ax = data["axes"]
    ok = ax["x"] == "x" and ax["y"] == "y"
    return ok, f"plotted axes = {ax} (the test pinned x_var=x, y_var=y)"


def _grid_present(model, data):
    ok = bool(_has_grid(data))
    return ok, (f"status={data.get('status')} — a 2-D random walk DOES dwell (Gaussian density), so "
                f"the occupancy heatmap must produce a grid, not an N/A card. na_reason="
                f"{data.get('na_reason')!r}")


def _peak_at_origin(model, data):
    if not _has_grid(data):
        return False, "no grid (status != ok) — cannot have a peak at the origin"
    p = data["peak"]
    ok = p is not None and abs(p["x"]) <= _PEAK_TOL and abs(p["y"]) <= _PEAK_TOL
    return ok, (f"peak cell = ({p['x']:.2f}, {p['y']:.2f}) count={p['count']}; expected ≈(0,0) "
                f"(the walk dwells most at its origin) — within ±{_PEAK_TOL}")


def _mean_at_origin(model, data):
    if not _has_grid(data):
        return False, "no grid (status != ok) — cannot check the centroid"
    mu = data["mean"]
    ok = abs(mu["x"]) <= _MEAN_TOL and abs(mu["y"]) <= _MEAN_TOL
    return ok, (f"centroid = ({mu['x']:.2f}, {mu['y']:.2f}); a 2-D RW's mean position stays at the "
                f"origin for all time — expected ≈(0,0) within ±{_MEAN_TOL}")


def _isotropic(model, data):
    if not _has_grid(data):
        return False, "no grid (status != ok) — cannot check isotropy"
    sx, sy = data["spread"]["x"], data["spread"]["y"]
    lo, hi = sorted((sx, sy))
    ratio = (hi / lo) if lo > 0 else float("inf")
    ok = ratio <= _ISO_RATIO
    return ok, (f"spread = (x={sx:.2f}, y={sy:.2f}), max/min ratio={ratio:.1f}; the two axes are "
                f"statistically identical, so a true RW density is ~isotropic (ratio ≤ {_ISO_RATIO})")


def _two_dimensional(model, data):
    if not _has_grid(data):
        return False, "no grid (status != ok)"
    sx, sy = data["spread"]["x"], data["spread"]["y"]
    ok = sx > 0 and sy > 0
    return ok, (f"spread = (x={sx:.2f}, y={sy:.2f}); occupancy must spread on BOTH axes (a 2-D walk "
                f"is not confined to a line)")


def _symmetric_about_origin(model, data):
    """The density at (a,b) should ≈ the density at (−a,−b): point-symmetry through the origin. We
    measure it as |Σ counts in the (x>0) half − Σ in the (x<0) half| relative to the total, and the
    same for y. A true RW density is balanced; the BFS-spray regression piles all mass on one side."""
    if not _has_grid(data):
        return False, "no grid (status != ok) — cannot check symmetry"
    g = data["grid"]
    xc, yc, counts = g["x_centers"], g["y_centers"], g["counts"]
    total = sum(sum(row) for row in counts) or 1.0
    xpos = sum(counts[i][j] for i in range(len(xc)) for j in range(len(yc)) if xc[i] > 0)
    xneg = sum(counts[i][j] for i in range(len(xc)) for j in range(len(yc)) if xc[i] < 0)
    ypos = sum(counts[i][j] for i in range(len(xc)) for j in range(len(yc)) if yc[j] > 0)
    yneg = sum(counts[i][j] for i in range(len(xc)) for j in range(len(yc)) if yc[j] < 0)
    imbx = abs(xpos - xneg) / total
    imby = abs(ypos - yneg) / total
    ok = imbx <= 0.5 and imby <= 0.5     # loose: a perfect split is 0; a one-sided spray is ~1
    return ok, (f"half-mass imbalance: x={imbx:.2f}, y={imby:.2f} (0 = perfectly symmetric, 1 = all "
                f"mass on one side); a RW density is symmetric about the origin — expected ≤ 0.5")


CHECKS = [
    Check("axes pinned to x,y (explicit x_var/y_var honored)", _axes_pinned),
    Check("produces an occupancy grid (a RW dwells — not N/A)", _grid_present),
    Check("peak cell at the origin (where the walk dwells most)", _peak_at_origin),
    Check("centroid at the origin (mean position stays at 0)", _mean_at_origin),
    Check("isotropic spread (the two axes are identical)", _isotropic),
    Check("non-degenerate 2-D spread (not a line)", _two_dimensional),
    Check("density symmetric about the origin", _symmetric_about_origin),
]


def case():
    return run_case("random_walk", SOURCE, "occupancy_heatmap", CHECKS, x_var="x", y_var="y")
