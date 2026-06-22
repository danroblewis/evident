#!/usr/bin/env python3
"""fixedpoint_attractors.py — attractor detection for render_fixedpoint_map.py.

The pure dynamics/analysis layer: given a loaded Evident IR `model` (with
.state_vars / .successor(s) / .successors(s)), find the fixed points and the
short / limit cycles its trajectories settle onto. No plotting lives here — the
renderer in render_fixedpoint_map.py imports these and draws the result.

`ordinal` (the value -> float projection) lives here too because the seed
spread depends on it; the renderer imports it back for its own axis projection.
"""


# --------------------------------------------------------------------------
# projection: a state value -> float coordinate
# --------------------------------------------------------------------------
def ordinal(model, var, value):
    """Map any value to a float coordinate for plotting."""
    k = var["kind"]
    if k in ("int", "real"):
        return float(value)
    if k == "bool":
        return 1.0 if value else 0.0
    if k == "enum":
        return float(model.enum_variants[var["name"]].index(value))
    if k == "string":
        return float(hash(value) % 997)
    return 0.0


# --------------------------------------------------------------------------
# attractor detection
# --------------------------------------------------------------------------
def state_key(model, st):
    return tuple(st[v["name"]] for v in model.state_vars)


def near(model, a, b, tol):
    """Approximate equality: exact on discrete axes, within tol on numeric."""
    for v in model.state_vars:
        av, bv = a[v["name"]], b[v["name"]]
        if v["kind"] in ("int", "real"):
            if abs(float(av) - float(bv)) > tol:
                return False
        else:
            if av != bv:
                return False
    return True


def is_absorbing(model, s, tol):
    """A genuine resting state: EVERY successor is (approximately) s itself.
    A self-loop that ALSO has other exits is not 'at rest' — it can leave."""
    try:
        succs = model.successors(s, limit=8)
    except Exception:
        one = model.successor(s)
        succs = [one] if one is not None else []
    if not succs:
        return False
    return all(near(model, s, t, tol) for t in succs)


def find_cycle_from(model, s, tol, max_len):
    """Follow ONE deterministic successor chain from s. If it returns near an
    earlier chain node, return that loop [a, b, ..., a] (period >= 2). Else None.
    `max_len` bounds the chain so long numeric limit cycles still close."""
    chain = [s]
    cur = s
    for _ in range(max_len):
        nxt = model.successor(cur)
        if nxt is None:
            return None
        for j, c in enumerate(chain):
            if near(model, nxt, c, tol):
                loop = chain[j:] + [chain[j]]
                return loop if len(loop) >= 3 else None  # period >= 2
        chain.append(nxt)
        cur = nxt
    return None


def find_attractors(model, states, mode):
    """Returns (fixed_points, cycles).

    fixed_points: absorbing states (every successor maps back to the state).
    cycles: distinct short/limit cycles [s0, ..., s0] (period >= 2).

    Discrete: scan reachable states with exact equality. Numeric: scan grid
    seeds with a coarse tolerance, and allow long chains so the limit cycle —
    whose per-tick step is small — has room to close."""
    # Cycle-closing needs slack on a coarse grid; a FIXED point must truly not
    # move (step ~ 0), so it gets a tight tolerance regardless of mode.
    cyc_tol = 30.0 if mode == "grid" else 0.0
    fix_tol = 1.0 if mode == "grid" else 0.0
    max_len = 360 if mode == "grid" else 40

    fixed = []
    cycles = []
    seen_cycle_keys = set()

    # Deep-probing every grid point for a long chain is expensive; for numeric
    # systems a handful of well-placed seeds reveal the same limit cycle.
    if mode == "grid":
        seeds = pick_numeric_seeds(model, states)
    else:
        seeds = states

    for s in states:
        if is_absorbing(model, s, fix_tol):
            fixed.append(s)

    for s in seeds:
        if any(near(model, s, f, fix_tol) for f in fixed):
            continue
        loop = find_cycle_from(model, s, cyc_tol, max_len)
        if loop is None:
            continue
        # dedupe cycles by their member set (coarsened on numeric axes)
        key = frozenset(coarse_key(model, c, cyc_tol) for c in loop[:-1])
        if key in seen_cycle_keys:
            continue
        seen_cycle_keys.add(key)
        cycles.append(loop)

    # Numeric systems whose orbits spiral onto an attractor (van der Pol) only
    # close after a long transient + full period — too long for the per-seed
    # chain above. Extract the limit cycle directly: run one long trajectory,
    # drop the transient, and take the SETTLED tail as the orbit.
    if mode == "grid" and not cycles:
        orbit = extract_limit_cycle(model, seeds, fixed, fix_tol)
        if orbit is not None:
            cycles.append(orbit)

    return fixed, cycles


def extract_limit_cycle(model, seeds, fixed, fix_tol):
    """Run a long trajectory from a mid-radius seed; if it settles onto a
    recurring orbit (tail returns near an earlier tail point), return that
    closed orbit. Returns a loop [p0, ..., p0] or None."""
    import math
    candidates = [s for s in seeds
                  if not any(near(model, s, f, fix_tol) for f in fixed)]
    for seed in candidates:
        cur = seed
        chain = [cur]
        for _ in range(700):
            nxt = model.successor(cur)
            if nxt is None:
                break
            chain.append(nxt)
            cur = nxt
        if len(chain) < 200:
            continue
        # search the settled tail for a near-recurrence (a closed loop)
        tail_start = int(len(chain) * 0.45)
        best = None
        for i in range(len(chain) - 1, tail_start + 30, -1):
            for j in range(tail_start, i - 30):
                d = _numeric_dist(model, chain[i], chain[j])
                if d <= 40.0:
                    best = (j, i)
                    break
            if best:
                break
        if best:
            j, i = best
            loop = chain[j:i] + [chain[j]]
            if len(loop) >= 4:
                return loop
    return None


def _numeric_dist(model, a, b):
    import math
    s = 0.0
    for v in model.state_vars:
        if v["kind"] in ("int", "real"):
            s += (float(a[v["name"]]) - float(b[v["name"]])) ** 2
    return math.sqrt(s)


def coarse_key(model, st, tol):
    parts = []
    q = max(tol, 1.0)
    for v in model.state_vars:
        val = st[v["name"]]
        if v["kind"] in ("int", "real"):
            parts.append(round(float(val) / q))
        else:
            parts.append(val)
    return tuple(parts)


def pick_numeric_seeds(model, states):
    """A spread of seeds across the scanned box: a ring of mid-radius points
    (likely to land in the limit-cycle basin) plus a few near-origin points
    (to catch a central fixed point's basin)."""
    numeric = [v for v in model.state_vars if v["kind"] in ("int", "real")]
    if len(numeric) < 2:
        return states[: min(len(states), 60)]
    import math
    xv, yv = numeric[0], numeric[1]
    base = max(abs(ordinal(model, xv, s[xv["name"]])) for s in states) or 3000.0
    seeds = []
    template = dict(states[0]) if states else {}
    for r in (0.15, 0.5, 0.85):
        for k in range(8):
            a = 2 * math.pi * k / 8
            st = dict(template)
            st[xv["name"]] = int(r * base * math.cos(a)) if xv["kind"] == "int" else r * base * math.cos(a)
            st[yv["name"]] = int(r * base * math.sin(a)) if yv["kind"] == "int" else r * base * math.sin(a)
            seeds.append(st)
    return seeds
