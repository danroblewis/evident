#!/usr/bin/env python3
"""cobweb_sample.py — channel selection + map/orbit sampling for render_cobweb.py.

The DATA layer for the cobweb plot: which scalar the cobweb is over (_pick_primary)
and the facet var, the ordinal<->value plumbing, the honest orbit-derived x-range,
the set-valued map + staircase samplers, and the non-autonomy / degeneracy guards.
No plotting policy lives here — render_cobweb.py imports these and draws.
"""

def _pick_primary(m):
    """Return (var, mode): the scalar the cobweb is OVER.

    Prefer the top-ranked NUMERIC var (true 1-D map); fall back to the top
    ranked var as an enum ordinal. (None, None) only if there are no vars."""
    if m.numeric_vars:
        return m.numeric_vars[0], "int"
    for v in m.state_vars:
        if v["kind"] == "enum":
            return v, "enum-ordinal"
    return (m.state_vars[0], "enum-ordinal") if m.state_vars else (None, None)


def _distinct_facet_groups(m, var, mode, base, grid, facet):
    """Group facet values by the map they produce, keeping insertion order. Returns
    a list of (representative_values, fingerprint): values sharing a fingerprint are
    collapsed into one group (one panel labelled with all of them). A facet whose
    values ALL collapse to one group adds no information and should be dropped."""
    def facet_values(fv):
        if fv["kind"] == "enum":
            return list(m.enum_variants[fv["name"]])
        return [False, True]

    def fingerprint(panel_base):
        # Canonical fingerprint of x_{n+1} = f(x_n) under panel_base. Two facet
        # values that yield the SAME fingerprint produce IDENTICAL cobweb scatters
        # — holding that var doesn't enter f, so a panel per value is a duplicate
        # (the `find` bug: s5=Unseen and s5=Visited are pixel-identical). Dedup
        # facet values before drawing.
        xs, ys = _sample_map(m, var, mode, panel_base, grid)
        return tuple(sorted(zip(xs, ys)))

    groups = []          # [(values, fingerprint)]
    index = {}           # fingerprint -> position in groups
    for val in facet_values(facet):
        panel_base = dict(base)
        panel_base[facet["name"]] = val
        fp = fingerprint(panel_base)
        if fp in index:
            groups[index[fp]][0].append(val)
        else:
            index[fp] = len(groups)
            groups.append(([val], fp))
    return groups


# --------------------------------------------------------------------------
# State construction + ordinal <-> value plumbing.
# --------------------------------------------------------------------------
def _base_state(m):
    """A neutral state holding the non-primary, non-facet vars fixed."""
    init = m.initial_state()
    if init is not None:
        return dict(init)
    state = {}
    for v in m.state_vars:
        if v["kind"] == "int":
            state[v["name"]] = 0
        elif v["kind"] == "bool":
            state[v["name"]] = False
        elif v["kind"] == "enum":
            state[v["name"]] = m.enum_variants[v["name"]][0]
        elif v["kind"] == "real":
            state[v["name"]] = 0.0
        else:
            state[v["name"]] = ""
    return state


def _to_ord(m, var, value):
    if var["kind"] == "enum":
        return m.enum_variants[var["name"]].index(value)
    if var["kind"] == "bool":
        return 1 if value else 0
    return value


def _from_ord(m, var, o):
    if var["kind"] == "enum":
        variants = m.enum_variants[var["name"]]
        o = max(0, min(len(variants) - 1, int(round(o))))
        return variants[o]
    if var["kind"] == "bool":
        return bool(int(round(o)))
    return int(round(o))


def _numeric_range(m, var, base):
    """Determine the cobweb x-range for a numeric var FROM THE REAL ORBIT, never a
    hardcoded ±3000 box AND never the over-approximated reachable extent (both are
    fabrication: gridding values the staircase never visits invents a map continuum /
    y=x line the program never enters — the lru `k0` bug).

    Grid over the distinct values the primary var takes along the trajectory, padded
    by one step on each side so the map's local neighborhood is visible. When the orbit
    only ever sits at a couple of values (lru's `k0 ∈ {-1, 1}`), the grid is just those
    points — no fake continuum. Falls back to `axis_bounds` only when the orbit is empty
    (no trajectory), and to a wide window only when even that is unbounded.

    Returns (grid_values, is_bounded). Returns (None, None) when no honest range exists
    so the caller routes to the N/A path instead of fabricating."""
    name = var["name"]
    # The DISTINCT values the primary var actually takes along the REAL orbit — the
    # single successor-chain from `base`. axis_bounds/reachable over-approximate
    # (they leave the OTHER carried vars free, reporting every value the var COULD
    # take across all branches), so gridding them fabricates a y=x continuum the
    # program never enters (the lru `k0` bug). The orbit is the honest domain.
    traj = m.trajectory(start=base, steps=400)
    seen, s = [], set()
    for st in traj:
        v = st.get(name)
        if v is not None and v not in s:
            s.add(v)
            seen.append(v)
    orbit = sorted(seen)
    if orbit:
        olo, ohi = orbit[0], orbit[-1]
        span = ohi - olo
        # Grid the orbit's own extent, padded by ONE unit on each side (so the
        # neighborhood of each visited value shows), capped at a readable resolution.
        # No wide window: a 2-value orbit grids ~4 points, not 0..75.
        ilo, ihi = int(round(olo)) - 1, int(round(ohi)) + 1
        ispan = ihi - ilo
        if ispan <= 0:
            return [int(round(olo))], True
        if ispan <= 200:
            return list(range(ilo, ihi + 1)), True
        n = 161
        return [ilo + ispan * i // (n - 1) for i in range(n)], True

    # No orbit (no trajectory at all) — fall back to the reachable extent.
    bounds = m.axis_bounds(name, pad=0.0)
    if bounds is None:
        # genuinely unbounded continuous dynamics with no finite reachable sample:
        # the ONLY case a generous window is honest. (Rare — most numeric Evident
        # FSMs have a finite reachable set.)
        lo, hi, n = -3200, 3200, 121
        grid = [lo + (hi - lo) * i // (n - 1) for i in range(n)]
        return grid, False
    lo, hi = bounds
    ilo, ihi = int(round(lo)), int(round(hi))
    span = ihi - ilo
    if span <= 0:
        return [ilo], True
    if span <= 400:                          # bounded reachable counter: grid exactly
        return list(range(ilo, ihi + 1)), True
    # bounded but wide: sample the reachable extent at fixed resolution (no padding
    # beyond what axis_bounds already added).
    n = 161
    grid = [ilo + span * i // (n - 1) for i in range(n)]
    return grid, True


# --------------------------------------------------------------------------
# Map + staircase sampling (set-valued aware).
# --------------------------------------------------------------------------
def _sample_map(m, var, mode, base, grid):
    """Sample x_{n+1} in f(x_n) for x_n over `grid`. Uses successors() so the
    FAN of a nondeterministic map shows all branches. Returns parallel
    (xs, ys) in ordinal space."""
    name = var["name"]
    xs, ys = [], []
    for x in grid:
        state = dict(base)
        state[name] = _from_ord(m, var, x)
        for nxt in m.successors(state):
            xs.append(x)
            ys.append(_to_ord(m, var, nxt[name]))
    return xs, ys


def _staircase(m, var, mode, base, seed, steps=60):
    """A cobweb staircase orbit following one successor chain from `seed`."""
    name = var["name"]
    px, py = [], []
    x = seed
    px.append(x); py.append(x)            # start on the diagonal
    seen = set()
    for _ in range(steps):
        state = {**base, name: _from_ord(m, var, x)}
        nxt = m.successor(state)
        if nxt is None:
            break
        y = _to_ord(m, var, nxt[name])
        px.append(x); py.append(y)        # vertical to the map
        px.append(y); py.append(y)        # horizontal to the diagonal
        key = round(y, 6)
        if key in seen:
            break
        seen.add(key)
        x = y
    return px, py


def _seed_for(mode, lo, hi, bounded, base, var, m):
    """Seed the cobweb staircase from a REACHABLE state — the initial state's value
    of this var (clamped into the gridded range), never a fabricated wide start
    (the old `seed=2000` invented an orbit through a region the program never
    enters)."""
    seed = _to_ord(m, var, base[var["name"]])
    if mode == "int":
        return max(lo, min(hi, seed))
    return seed


def _reachable_count(m):
    """Number of distinct reachable states (capped) — for the degeneracy guard."""
    try:
        states, _ = m.reachable(limit=64)
        return len(states)
    except Exception:
        return None


def _held_alts(m, hv):
    """A few alternative previous-values for a HELD companion var, to perturb it."""
    if hv["kind"] == "enum":
        return list(m.enum_variants[hv["name"]])
    if hv["kind"] == "bool":
        return [False, True]
    # numeric companion: sample a handful of values it actually takes on the orbit. Must accept
    # float (Real) too — restricting to int silently returned [] for a continuous companion like
    # the oscillator's `pos`, so the non-autonomy probe never perturbed it and drew a misleading
    # 1-D slice of a genuinely 2-D coupled system (Marek #38).
    vals = sorted({s.get(hv["name"]) for s in m._sample_states()
                   if isinstance(s.get(hv["name"]), (int, float)) and not isinstance(s.get(hv["name"]), bool)})
    return vals[:4]


def _depends_on_held(m, var, base, grid):
    """Is the candidate scalar's successor a function of a HELD companion var, rather
    than a self-contained 1-D map?

    The cobweb scans x_n -> x_{n+1} with the OTHER carried vars pinned at `base`. That
    is only honest when f(x) = state.X(_state.X) — when the scalar's next value is
    determined by its OWN previous value alone. If instead the next value depends on a
    held companion (vending's balance(n+1) is driven by the held state.mode, not by
    balance(n)), the scanned map is a LIE: holding mode=Idle makes f(balance)=0 for
    EVERY balance, a flat line that wrongly implies balance always collapses to 0.

    Probe: for a few x_n on the grid, perturb each held companion ONE at a time
    (holding the scalar and the rest) and check whether the scalar's successor MOVES.
    If any perturbation changes it, the scalar is non-autonomous over these axes and
    the 1-D cobweb is not meaningful. Returns (True, (x, held_name, alt)) on the first
    witness, else (False, None)."""
    name = var["name"]
    # use ALL interface vars, not the DEDUPED state_vars — csv_stats' cursor/count/sum
    # are partition-equivalent on the trajectory so dedup collapses them, dropping cursor
    # (the real driver of sum) from the held set and hiding the non-autonomy.
    held = [v for v in m.interface_vars if v["name"] != name]
    if not held:
        return False, None
    # Probe bases: grid scan-points at the neutral base, PLUS a few REAL reachable
    # states. A single neutral base can sit where a companion's influence is masked
    # (e.g. csv_stats' sum is driven by the held cursor, but at a past-EOF/neutral
    # cursor the scalar is frozen so the dependence hides) — probing reachable states
    # catches it where the dependence is live.
    probes = []
    if grid:
        n = len(grid)
        for x in sorted({grid[0], grid[n // 2], grid[-1]}):
            st = dict(base)
            st[name] = _from_ord(m, var, x)
            probes.append(st)
    probes += m._sample_states()[:5]
    for st in probes:
        ref = m.successor(st)
        if ref is None:
            continue
        refv = ref.get(name)
        for hv in held:
            for alt in _held_alts(m, hv):
                if alt == st.get(hv["name"]):
                    continue
                pert = m.successor({**st, hv["name"]: alt})
                if pert is not None and pert.get(name) != refv:
                    return True, (st.get(name), hv["name"], alt)
    return False, None
