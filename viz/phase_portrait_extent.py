#!/usr/bin/env python3
"""phase_portrait_extent.py — deriving the HONEST field domain for a >=2-numeric program.

The phase portrait grids a vector field over a box in value-space. The whole question of
whether the picture is honest or fabricated is WHICH box — so this module owns the domain
derivation, separate from the drawing (`phase_portrait_field.py`):

  * `numeric_regime` classifies the program by its REACHABLE dynamics (finite march / bounded
    march / genuine continuum / degenerate), so the renderer picks the honest picture.
  * `reachable_extent` / `dwell_span` derive the box for a BOUNDED march from the states the
    program actually VISITS (its followed trajectory), not the relational reachable fan.
  * `orbit_extent` derives the box for a genuine continuum by PROBING perturbation orbits off
    the fixed point — preferring `proven_range` (the all-conditions box, when finitely bounded)
    and only the BOUNDED part of each orbit (a diverging seed contributes nothing).
  * `proven_axis_range` is the proven-bounds source.

All reads of the transition go through the divergence-guarded helpers in phase_portrait_guard,
so a runaway seed truncates instead of crashing. The value<->plane projection (`_numeric`,
`_is_numeric`) is imported from phase_portrait_field (its primary home)."""
from phase_portrait_field import _numeric, _is_numeric
from phase_portrait_guard import _DIVERGE, safe_trajectory


def proven_axis_range(m, var):
    """The PROVEN reachable [lo, hi] of a numeric axis var as floats (the all-conditions box
    `Model.proven_range` derives via z3 Optimize over the transition), or None when the var
    is non-numeric or unbounded. The same finite-box source render_time_series seeds its
    continuous ensemble from — so the field's domain is the proven reachable set, not a guess.
    A free-running oscillator (spring / lotka / vanderpol) has no finite optimum → None, and
    the caller falls back to the perturbation-orbit discovery."""
    if not _is_numeric(var):
        return None
    rng = m.proven_range(var)
    if rng is None:
        return None
    lo, hi = float(rng[0]), float(rng[1])
    return (lo, hi) if hi > lo else None


def _probe_orbit_bbox(m, axx, axy, pin, seed_floor):
    """Probe perturbation orbits off the fixed point and return the bbox of the BOUNDED parts
    they visit, as (xs, ys) extreme lists. Seeds scale up geometrically from `seed_floor` until
    the orbit stops expanding; only the part of each truncated orbit that stays under _DIVERGE
    sets the extent (lotka's far seeds blow up before settling — their runaway tail is dropped)."""
    nx_, ny_ = axx["name"], axy["name"]
    xs, ys = [], []
    prev_reach = -1.0
    for mult in (1, 4, 16, 64, 256):
        scale = seed_floor * mult
        seeds = [(scale, 0), (0, scale), (-scale, 0), (0, -scale),
                 (scale * 0.5, scale * 0.5)]
        ox, oy = [], []
        for sx0, sy0 in seeds:
            st = dict(pin)
            st[nx_] = int(round(sx0)) if axx["kind"] == "int" else sx0
            st[ny_] = int(round(sy0)) if axy["kind"] == "int" else sy0
            tr = safe_trajectory(m, st, 400)         # diverging seed truncates, never crashes
            for s in tr:
                sx, sy = _numeric(m, axx, s[nx_]), _numeric(m, axy, s[ny_])
                if abs(sx) > _DIVERGE or abs(sy) > _DIVERGE:
                    break                            # drop the runaway tail from the bbox
                ox.append(sx); oy.append(sy)
        if ox:
            xs += [min(ox), max(ox)]
            ys += [min(oy), max(oy)]
            reach = max(abs(min(ox)), abs(max(ox)), abs(min(oy)), abs(max(oy)))
            if reach <= prev_reach * 1.05:           # stopped expanding -> found its reach
                break
            prev_reach = reach
    return xs, ys


def orbit_extent(m, axx, axy, pin):
    """The bounding box of the actually-VISITED states for a continuous numeric field, derived
    from the program's reachable dynamics — NEVER a hardcoded box. Three reachable sources,
    unioned, in preference order:

      * proven_range: the PROVEN reachable box (z3 Optimize) when the axis is finitely bounded —
        the same all-conditions box render_time_series grids its continuous ensemble over.
      * axis_bounds: the padded extent over the reachable BFS sample.
      * perturbation orbits (`_probe_orbit_bbox`): seed a spread of off-origin starts and follow
        the successor chain; the bbox of the BOUNDED orbit IS the limit-cycle extent (vanderpol).

    `pin` fixes the non-axis carried vars while the two axes sweep. Returns (xlo,xhi,ylo,yhi)."""
    nx_, ny_ = axx["name"], axy["name"]
    xs, ys = [], []

    # proven_range (finite) is the most honest all-conditions domain; a free-running oscillator
    # returns None and we fall through to axis_bounds + the perturbation probe.
    px, py = proven_axis_range(m, axx), proven_axis_range(m, axy)
    if px is not None:
        xs += [px[0], px[1]]
    if py is not None:
        ys += [py[0], py[1]]
    bx, by = m.axis_bounds(nx_), m.axis_bounds(ny_)
    if bx is not None:
        xs += [bx[0], bx[1]]
    if by is not None:
        ys += [by[0], by[1]]

    seed_floor = max([abs(v) for v in xs + ys] + [1.0])
    ox, oy = _probe_orbit_bbox(m, axx, axy, pin, seed_floor)
    xs += ox
    ys += oy

    if not xs or not ys:
        return -10.0, 10.0, -10.0, 10.0
    xlo, xhi = min(xs), max(xs)
    ylo, yhi = min(ys), max(ys)
    # square + pad so the field reads symmetrically around the cycle
    span = max(abs(xlo), abs(xhi), abs(ylo), abs(yhi), 1.0) * 1.15
    return -span, span, -span, span


# A reachable set this small is a drawable transition GRAPH (points + arrows); above it the
# graph is an unreadable hairball and a magnitude-colored vector field over the bounded
# reachable domain is the honest picture instead.
_FINITE_STATE_CAP = 60


def numeric_regime(m, axx, axy):
    """Classify a >=2-numeric program by its REACHABLE dynamics, so we pick the honest picture
    instead of always gridding a guessed field:

      * "finite"     — small terminating reachable march, both axes moving. A drawable graph.
      * "bounded"    — a large/non-terminating march, both axes move AND axis_bounds is finite.
                       Grid a field over the reachable extent, never a fabricated box.
      * "continuous" — the reachable set is a lone fixed point but perturbation orbits trace a
                       non-trivial cycle (vanderpol's limit cycle). The orbit extent is honest.
      * "degenerate" — a constant axis, or a lone fixed point with no orbit. No 2D field.
    """
    nx_, ny_ = axx["name"], axy["name"]
    states, _ = m.reachable(limit=3000)

    dx = len({_numeric(m, axx, s[nx_]) for s in states}) if states else 0
    dy = len({_numeric(m, axy, s[ny_]) for s in states}) if states else 0

    # a MULTI-state march where one axis never moves = no 2D portrait. A SINGLE-state reachable
    # set (an unstable fixed point) is NOT this case — its orbit lives off the initial point, so
    # fall through to the probe (vanderpol). Guard on len(states) > 1 so we don't kill that.
    if len(states) > 1 and (dx <= 1 or dy <= 1):
        return "degenerate"
    if 2 <= len(states) <= _FINITE_STATE_CAP and dx > 1 and dy > 1:
        return "finite"

    # both axes move AND the reachable extent is bounded = a real but large march. A bounded
    # field is only honest if the actual DWELL moves on both axes (m.reachable() fans into a
    # relational cloud that can move an axis the real run holds fixed) — else report N/A.
    if dx > 1 and dy > 1:
        bx, by = m.axis_bounds(nx_), m.axis_bounds(ny_)
        if bx is not None and by is not None:
            dwell = dwell_span(m, axx, axy)
            if dwell is not None:
                (xlo, xhi), (ylo, yhi) = dwell
                if xhi - xlo < 1e-9 or yhi - ylo < 1e-9:
                    return "degenerate"
            return "bounded"

    # otherwise probe for a continuous orbit by perturbing off the initial state
    init = m.initial_state() or {v["name"]: 0 for v in m.state_vars}
    pin = {v["name"]: init[v["name"]] for v in m.state_vars}
    xlo, xhi, ylo, yhi = orbit_extent(m, axx, axy, pin)
    orbit_span = max(abs(xlo), abs(xhi), abs(ylo), abs(yhi))
    init_mag = max(abs(_numeric(m, axx, init[nx_])),
                   abs(_numeric(m, axy, init[ny_])), 1.0)
    if orbit_span > 4.0 * init_mag and orbit_span > 4.0:
        return "continuous"
    return "degenerate"


def dwell_span(m, axx, axy):
    """The per-axis extent of the states the program actually DWELLS in — its real deterministic
    trajectory — as ((xlo,xhi),(ylo,yhi)). The honest framing source for a bounded march:
    m.reachable() fans into a relational cloud that has nothing to do with where the program
    goes; the followed trajectory is the set the difference equation truly visits. Returns None
    if the trajectory is empty."""
    init = m.initial_state()
    tr = safe_trajectory(m, init, 400) if init is not None else []  # truncate on divergence
    if len(tr) < 2:
        return None
    out = []
    for v in (axx, axy):
        nm = v["name"]
        vals = sorted(_numeric(m, v, s[nm]) for s in tr)
        # Strip a lone off-domain visit by ISOLATION (the same gap-based test axis_bounds uses,
        # #465 follow-up) — NOT a 3×IQR quantile fence, which collapsed a SMOOTH decaying transient
        # (a spiral sink dwelling near 0) to ~1e-8 because the IQR shrinks to ~0 and the legitimate
        # early extent reads as an outlier. A dense transient keeps its full span; a real sentinel
        # is still peeled. Keeps the RAW span (no zero-width widening) so a genuinely-constant axis
        # still reports lo==hi for the caller's collapse check.
        vals = m._strip_isolated_sentinels(vals)
        out.append((float(min(vals)), float(max(vals))))
    return tuple(out)


def reachable_extent(m, axx, axy):
    """The field domain for a BOUNDED numeric march: the robust extent of the states the program
    actually VISITS (its followed trajectory), lightly padded. NOT axis_bounds (the relational
    fan blows the frame out to a mostly-empty box). Falls back to axis_bounds only when no
    trajectory is available. Returns (xlo,xhi,ylo,yhi)."""
    dwell = dwell_span(m, axx, axy)
    if dwell is not None:
        (xlo, xhi), (ylo, yhi) = dwell
    else:
        bx = m.axis_bounds(axx["name"]) or (0.0, 1.0)
        by = m.axis_bounds(axy["name"]) or (0.0, 1.0)
        xlo, xhi, ylo, yhi = bx[0], bx[1], by[0], by[1]
    px = max((xhi - xlo) * 0.08, 0.5)
    py = max((yhi - ylo) * 0.08, 0.5)
    return xlo - px, xhi + px, ylo - py, yhi + py
