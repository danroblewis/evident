"""model_const — constants shared by the Model core and its analysis/query layers.

Pulled into their own module so `model_analysis` / `model_query` can import them
WITHOUT importing `evident_viz` (which imports the mixin classes from those modules
at class-definition time — a back-dependency on evident_viz would be circular).
"""

# Per-solve wall-clock cap (ms). Every z3 Solver/Optimize the dynamics layer builds gets this, so a
# single intractable check — e.g. an NRA reachable-step on a nonlinear-Real sample (predator-prey's
# _prey·_pred) — returns `unknown` instead of hanging the whole server unboundedly (Ana #300). A timed
# check that returns unknown is treated exactly like unsat (no successor), so sampling stops cleanly.
SOLVE_TIMEOUT_MS = 4000


# Visual-channel effectiveness by variable class (Cleveland & McGill 1984 /
# Mackinlay 1986): POSITION decodes best for everything; SIZE is good for
# quantitative but poor for categorical; COLOR (hue) and FACET are excellent for
# categorical but weak for quantitative. importance(var) x this table decides which
# variable lands on which channel. Color/size/facet are SECONDARY — a good plot
# reads from its axes alone.
CHANNEL_FITNESS = {
    "x":       {"quant": 1.00, "cat": 0.90},
    "y":       {"quant": 1.00, "cat": 0.90},
    "size":    {"quant": 0.70, "cat": 0.25},
    "opacity": {"quant": 0.60, "cat": 0.25},
    "color":   {"quant": 0.40, "cat": 0.85},
    "facet":   {"quant": 0.20, "cat": 0.80},
    "shape":   {"quant": 0.10, "cat": 0.60},
}


def robust_value_band(vals, plo=1.0, phi=99.0, grow=50.0):
    """The [lo, hi] of a numeric sample with a DIVERGENT tail trimmed off — states whose magnitude
    runs away GEOMETRICALLY (explicit-Euler overshoot on an unstable scheme: Lotka-Volterra
    spiralling out to ±1e18). Those states are NUMERICAL ARTIFACTS, not the orbit: left in, they
    (a) fabricate a reported reachable bound the real orbit never occupies and (b) blow a phase-
    portrait axis to 1e18, flattening the field to one line (#484). A LONE sentinel is caught by the
    gap test (_strip_isolated_sentinels); a divergent CLUSTER — even one densely traced by a runaway
    trajectory — needs this.

    Detector — magnitude relative to the percentile ANCHOR, NOT to the bulk centre. A geometric
    divergence makes max ≫ p99 by many ORDERS (LV: max 1e18 vs p99 1750); a smooth orbit's max sits
    within a small multiple of p99 (a spiral SINK decaying 1→0 has p99 ≈ its real peak 0.2, max 1.0
    — ratio ~5×, NOT a blowup). So fence at p99 + `grow`·|anchor| (and symmetrically below p1): a
    value past that is a runaway and is dropped. Anchoring on |p99|/|p1| MAGNITUDE — not the inner
    span — is the crux: a converging orbit's centre is ~0, so a span/IQR/MAD anchor would collapse
    and clip the real transient (the #465 trap); the percentile MAGNITUDE survives it. A bounded
    spread (uniform, counter), a decay, a growing-but-finite orbit, and a bimodal set are all
    returned untouched (band == raw extent); only a true ±1e18-class runaway is shed.

    SOUND-BY-CONSTRUCTION: the band is the SINGLE region both the reported reachable bound AND the
    plotted/hoverable phase-portrait points pass through, so a displayed state can never fall outside
    the bound it claims (bound ⊇ points). Returns (lo, hi); raw (min, max) when no runaway is found
    or there's too little data. Never inverted."""
    xs = sorted(float(v) for v in vals if isinstance(v, (int, float)) and not isinstance(v, bool))
    n = len(xs)
    if n < 8:
        return (xs[0], xs[-1]) if xs else (0.0, 1.0)

    def pct(p):
        r = (p / 100.0) * (n - 1)
        i = int(r)
        return xs[i] if i + 1 >= n else xs[i] * (1 - (r - i)) + xs[i + 1] * (r - i)

    qlo, qhi = pct(plo), pct(phi)
    scale = max(abs(qlo), abs(qhi), 1e-12)         # anchor MAGNITUDE — robust to a ~0-centred orbit
    fence_lo, fence_hi = qlo - grow * scale, qhi + grow * scale
    kept = [v for v in xs if fence_lo <= v <= fence_hi]
    if not kept:
        return (xs[0], xs[-1])
    return (kept[0], kept[-1])


def widen_bounds_to_points(structure, points):
    """Widen a `structure.bounds` map so it CONTAINS every response hover-point (#490): for each
    bounded axis, extend [lo, hi] to also cover the min/max that axis takes across `points[].state`,
    rounding OUTWARD (floor lo, ceil hi to 3 dp) so a rounded edge never clips the very point it was
    widened to include. The reported bound (a robust reachable range) and the points (the phase-
    portrait's seeded orbits, which swing wider than from-init) come from different samplers; this
    reconciles them so the header chip never advertises a range a returned point falls outside —
    bound ⊇ points by construction. Returns the structure with a reconciled bounds dict (shallow
    copy; structure otherwise untouched); unchanged when there's no bounds dict or no points."""
    import math
    if not isinstance(structure, dict):
        return structure
    bounds = structure.get("bounds")
    if not bounds or not points:
        return structure
    short = lambda n: n.split(".")[-1]
    new_bounds = dict(bounds)
    for pt in points:
        for name, val in (pt.get("state") or {}).items():
            sk = short(name)
            if sk in new_bounds and isinstance(val, (int, float)) and not isinstance(val, bool):
                lo, hi = new_bounds[sk]
                if val < lo:
                    lo = math.floor(val * 1000) / 1000
                if val > hi:
                    hi = math.ceil(val * 1000) / 1000
                new_bounds[sk] = [lo, hi]
    out = dict(structure)
    out["bounds"] = new_bounds
    return out
