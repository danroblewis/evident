#!/usr/bin/env python3
"""nullcline_analysis.py — recurrent-extent + sign-grid analysis for
render_nullcline_field.py.

The DATA layer: follow the transition relation to decide whether the two axes
carry a genuine 2-D recurrent vector field, derive the honest plotting window
from the reachable / recurrent extent (never a hardcoded box), and sample the
sign-of-change grid. No plotting policy lives here.
"""
import numpy as np

# Padding beyond the reachable-derived range for the sign-field window.
PAD = 1.10

# Below this many distinct reachable states with no recurrent orbit, a sign-field
# would fabricate sign-regions the program never enters — route to N/A.
MIN_FIELD_POINTS = 24

# Escalating perturbation magnitudes used ONLY to recover the recurrent extent of
# a continuous oscillator whose initial state is a fixed point (e.g. vanderpol's
# origin). These are kicks to escape the fixed point, NOT a plotting box; the grid
# extent is read off the orbit the transition relation produces.
_KICKS = [4, 16, 64, 256, 1024, 4096]


def _orbit_pts(m, xv, vv, seed, steps=600):
    """Follow the transition relation from `seed`, collecting visited (x, v)
    points. The orbit is whatever m.successor produces — never hardcoded."""
    xn, vn = xv["name"], vv["name"]
    cur = {xn: int(seed[0]), vn: int(seed[1])}
    pts = []
    for _ in range(steps):
        nxt = m.successor(cur)
        if nxt is None:
            break
        cur = {xn: nxt[xn], vn: nxt[vn]}
        pts.append((cur[xn], cur[vn]))
    return pts


def _is_recurrent_field(m, xv, vv):
    """Does the program's OWN trajectory exhibit genuine 2D recurrent motion on
    these two axes — a real vector field — or is it a monotone counter / clock /
    constant that merely SPANS a wide reachable box?

    A sign-field is only honest when the dynamics on (xv, vv) loop back: an
    oscillator revisits cells (vanderpol's limit cycle), a basin spirals in. A
    monotone counter (life's `gen`, randomwalk's visit-counters v3/v4) marches
    away forever and NEVER revisits — gridding it carpets the plane with one
    uniform sign-region and fabricates a field the program never lives on.

    Honest signals that disqualify a sign-field here:
      * EITHER axis is constant on the reachable set (life's pop ≡ 3) — there is
        no second continuous coordinate, so it is not a plane.
      * the trajectory is strictly monotone on BOTH axes and never revisits a
        cell — a clock/counter pair, not a flow with structure.
    Returns True only when the axes carry real recurrent / non-monotone motion."""
    states, _ = m.reachable(limit=3000)
    xn, vn = xv["name"], vv["name"]
    xvals = {s[xn] for s in states if xn in s}
    vvals = {s[vn] for s in states if vn in s}
    if len(xvals) <= 1 or len(vvals) <= 1:
        return False                      # a constant axis → not a 2D plane

    traj = m.trajectory(steps=600)
    if len(traj) < 4:
        return False
    xs = [s[xn] for s in traj if xn in s]
    vs = [s[vn] for s in traj if vn in s]
    cells = list(zip(xs, vs))
    distinct = len(set(cells))
    revisits = len(cells) - distinct
    # A revisit only means RECURRENCE if the orbit traverses several distinct
    # cells before returning (a genuine loop). An orbit frozen on ONE cell
    # (randomwalk's deterministic trajectory sits at (0,0) forever) revisits
    # constantly but is a stuck fixed point, not a field — reject it.
    if revisits > 0 and distinct >= 4:
        return True                       # the orbit loops back — a real cycle

    def _monotone(seq):
        nd = all(seq[i] <= seq[i + 1] for i in range(len(seq) - 1))
        ni = all(seq[i] >= seq[i + 1] for i in range(len(seq) - 1))
        return nd or ni
    # No revisit AND each axis only ever moves one way → counter/clock, not a field.
    if _monotone(xs) and _monotone(vs):
        return False
    return True


def _recurrent_extent(m, xv, vv, seeds):
    """If the relation has a recurrent set (a limit cycle) reachable by kicking
    OFF a fixed point, return its bounding box read off the orbit's second half
    (the transient settles into the cycle). None if no bounded recurrence found.

    This is the honest extent for a continuous oscillator whose initial state is
    a fixed point: the ±2030 of vanderpol EMERGES from solving the transition, it
    is not a guessed box."""
    best = None
    for s in seeds:
        pts = _orbit_pts(m, xv, vv, s)
        if len(pts) < 8:
            continue
        tail = pts[len(pts) // 2:]
        if len(tail) < 4:
            continue
        xs = [p[0] for p in tail]
        vs = [p[1] for p in tail]
        spanx, spanv = max(xs) - min(xs), max(vs) - min(vs)
        # require a genuine recurrent loop, not a stuck point or a runaway
        if spanx < 2 and spanv < 2:
            continue
        box = (min(xs), max(xs), min(vs), max(vs))
        # keep the largest stable recurrent box seen across kicks
        if best is None or (spanx + spanv) > (best[1] - best[0]) + (best[3] - best[2]):
            best = box
    return best


def axis_extent(m, xv, vv):
    """Derive the sign-field plotting window from the program's REACHABLE /
    visited states — never a hardcoded ±3000 box (the fabrication bug).

    Domain sources, in order of honesty:
      1. axis_bounds over the reachable sample (the real visited extent).
      2. for an oscillator whose reachable set collapses to a fixed point, the
         recurrent (limit-cycle) extent read off the transition's own orbit.

    Returns (xlo, xhi, vlo, vhi) or None when the reachable structure is too
    small / finite for a sign-field to be meaningful — the caller then routes to
    an honest N/A rather than carpeting a guessed plane."""
    states, _ = m.reachable(limit=3000)
    bx = m.axis_bounds(xv["name"])
    bv = m.axis_bounds(vv["name"])

    have_reach = bx is not None and bv is not None
    reach_box = None
    if have_reach:
        reach_box = (bx[0], bx[1], bv[0], bv[1])

    # Is the reachable set a real continuous structure, or a handful of points?
    n_states = len({(s.get(xv["name"]), s.get(vv["name"])) for s in states})
    reach_span = 0.0
    if reach_box:
        reach_span = (reach_box[1] - reach_box[0]) + (reach_box[3] - reach_box[2])

    # Probe for a recurrent orbit ONLY when the reachable set is a single fixed
    # point — the signature of a continuous oscillator (e.g. vanderpol's origin)
    # whose limit cycle isn't reached from the seeded initial state. A program
    # with a real multi-state reachable trajectory that simply TERMINATES (wc:
    # 11 states in 0..10) must NOT be kicked off-domain — that would fabricate a
    # cycle from out-of-domain prev-states. Kicks derive from the fixed point +
    # escalating perturbations, NOT a fixed ±3200 spread.
    rec_box = None
    if n_states <= 1:
        cx = 0.5 * (reach_box[0] + reach_box[1]) if reach_box else 0.0
        cv = 0.5 * (reach_box[2] + reach_box[3]) if reach_box else 0.0
        seeds = []
        for k in _KICKS:
            seeds += [(cx + k, cv), (cx, cv + k), (cx + k, cv + k)]
        rec_box = _recurrent_extent(m, xv, vv, seeds)

    # Choose the domain: prefer a real recurrent orbit if it's larger than the
    # reachable extent (the limit cycle the program lives on); else the reachable
    # extent; degenerate-and-no-cycle -> None (honest N/A).
    box = None
    if rec_box and ((rec_box[1] - rec_box[0]) + (rec_box[3] - rec_box[2]) > reach_span):
        box = rec_box
    elif have_reach and n_states >= MIN_FIELD_POINTS and _is_recurrent_field(m, xv, vv):
        box = reach_box
        # NB: a wide value RANGE is not enough — a finite terminating trajectory
        # (csv_stats' sum 0..242, ls' sizes to 65536) is not a continuous vector
        # field however wide it spans; and a monotone counter/clock pair (life's
        # gen×pop, randomwalk's visit-counters v3×v4) spans a huge reachable box
        # but NEVER recurs — gridding it carpets the plane with one fabricated
        # sign-region. Only a genuinely recurrent / non-monotone reachable set
        # (_is_recurrent_field) earns a sign-field; else we route to N/A below.

    if box is None:
        return None

    xlo, xhi, vlo, vhi = box

    def pad(lo, hi):
        c = 0.5 * (lo + hi)
        r = max(1.0, 0.5 * (hi - lo)) * PAD
        return c - r, c + r
    return (*pad(xlo, xhi), *pad(vlo, vhi))


def _sign_grid(m, xv, vv, xs, vs):
    DX = np.full((len(vs), len(xs)), np.nan)
    DV = np.full((len(vs), len(xs)), np.nan)
    for j, vval in enumerate(vs):
        for i, xval in enumerate(xs):
            st = {xv["name"]: int(round(xval)), vv["name"]: int(round(vval))}
            nxt = m.successor(st)
            if nxt is None:
                continue
            DX[j, i] = nxt[xv["name"]] - st[xv["name"]]
            DV[j, i] = nxt[vv["name"]] - st[vv["name"]]
    return DX, DV
