#!/usr/bin/env python3
"""scatter_sample.py — the state-sampling DATA layer for render_scatter_matrix.py.

Answers "which states does the program visit, that the scatter matrix should
plot?" with no plotting policy. The renderer imports `sample_states`; everything
here is pure queries against the shared evident_viz Model (`m.reachable`,
`m.trajectory`, `m.successor`, `m.axis_bounds`). Split out of render_scatter_matrix
so the renderer holds only the draw code (and stays under the file-length bar).
"""


def sample_states(m):
    """A cloud of states drawn from the program's REACHABLE set + a parallel list
    of (i,j) edges (as index pairs into the cloud) when cheaply available.
    Returns (states, edges_or_None).

    The cloud is always anchored to states the program ACTUALLY visits — never a
    hardcoded ±3000 box. For discrete programs this is the exact reachable graph;
    for numeric ones it's the reachable cloud (capped) plus a long trajectory for
    the attractor / limit cycle. Any supplementary grid sweep is confined to the
    REACHABLE extent (m.axis_bounds), so the off-diagonal panels show the real
    vector field over the domain the program enters, not invented structure over a
    guessed plane."""
    # Reachable graph: exact for discrete, the true visited cloud for numeric. A scatter cloud reads
    # the same at ~800 points as at 5000 — overplotting adds nothing but seconds (reachable(5000) on a
    # 4-var FSM was ~6s, the dominant cost of that analyze; #217). Cap the cloud; the boundary box +
    # the analyze's own bounds still convey the full extent.
    states, edges = m.reachable(limit=800)
    if m.is_discrete():
        return states, edges

    # Numeric / mixed. The reachable-from-init cloud is the ground truth, but for a
    # continuous oscillator the init may sit at a fixed point whose basin is tiny
    # (e.g. van der Pol relaxes to (0,0) from the origin while the limit cycle lives
    # far out). So we ALSO probe the attractor with trajectories seeded off the
    # fixed point — and we scale everything to the extent those trajectories
    # actually trace (the limit-cycle extent), never a hardcoded ±3000 box.
    if not states:
        states = []
    edges = None

    init = m.initial_state()
    for st in m.trajectory(start=init, steps=400):
        states.append(st)

    int_vars = [v for v in m.state_vars if v["kind"] in ("int", "real")]

    def extent(name):
        vals = [s[name] for s in states if type(s.get(name)) in (int, float)]
        return (min(vals), max(vals)) if vals else (0.0, 0.0)

    # Does the cloud we have ALREADY span a real domain? If the reachable set has
    # genuine variation (a terminating counter visits a dozen distinct states), THAT
    # is the whole truth — plot it directly, no sweep, no invented structure.
    if len(int_vars) >= 2:
        a, b = int_vars[0]["name"], int_vars[1]["name"]
        ax_lo, ax_hi = extent(a)
        bx_lo, bx_hi = extent(b)
        degenerate = (ax_hi - ax_lo) < 1e-6 and (bx_hi - bx_lo) < 1e-6
    else:
        degenerate = False

    if not degenerate:
        # The reachable cloud is the real, fully-enumerated picture (or there's only
        # one numeric axis). Don't sweep — sweeping a finite program's lattice
        # fabricates states it never enters. Return what's actually visited.
        return states, edges

    a, b = int_vars[0]["name"], int_vars[1]["name"]
    return _probe_degenerate_attractor(m, states, init, a, b), edges


def _probe_degenerate_attractor(m, states, init, a, b):
    """Grow the `states` cloud for a degenerate fixed-point at a continuous system's
    init (e.g. van der Pol relaxes to (0,0) from the origin while the limit cycle
    lives far out). Probes outward on a geometric ladder of off-origin seeds to
    capture the attractor, then sweeps the vector field over the DISCOVERED extent —
    never a hardcoded ±3000 box. Mutates and returns `states`."""
    def extent(name):
        vals = [s[name] for s in states if type(s.get(name)) in (int, float)]
        return (min(vals), max(vals)) if vals else (0.0, 0.0)

    for scale in (1, 4, 16, 64, 256, 1024):
        for (sx, sy) in [(scale, 0), (0, scale), (-scale, scale), (scale, scale)]:
            traj = m.trajectory(start={**init, a: sx, b: sy}, steps=400)
            if len(traj) > 2:
                states.extend(traj)
    ax_lo, ax_hi = extent(a)
    bx_lo, bx_hi = extent(b)

    # Vector-field sweep, confined to the DISCOVERED attractor extent (never ±3000).
    # Only when even the attractor probe found nothing finite do we fall back to a
    # default wide box (genuinely unbounded continuous dynamics with no orbit).
    def axis_grid(lo, hi, default_name, n=9):
        if hi <= lo + 1e-9:
            bnds = m.axis_bounds(default_name)
            lo, hi = bnds if bnds is not None else (-3000.0, 3000.0)
        if hi <= lo:
            return [lo]
        step = (hi - lo) / (n - 1)
        return [lo + step * k for k in range(n)]

    gx, gy = axis_grid(ax_lo, ax_hi, a), axis_grid(bx_lo, bx_hi, b)
    base = init.copy()
    for x in gx:
        for y in gy:
            s = base.copy()
            s[a] = x
            s[b] = y
            nxt = m.successor(s)
            if nxt is not None:
                states.append(s)
                states.append(nxt)
    return states
