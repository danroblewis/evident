#!/usr/bin/env python3
"""basin_domain.py — the numeric/seed DOMAIN derivation for the basin map.

Everything that answers "over what extent should we grid/seed, given what the
program actually reaches?" lives here, split out of basin_numeric.py: the
per-var baseline, the reachable/visited-set domain, the off-init probe widening
(van der Pol's unstable origin), and the per-axis grid. NEVER a hardcoded box —
the domain is what the dynamics touch.
"""
import numpy as np


def baseline_fn(m):
    """Baseline value for a non-axis var: initial_state if present, else a neutral
    default (0 / false / first variant / "")."""
    init = m.initial_state()

    def baseline(v):
        if init is not None and v["name"] in init:
            return init[v["name"]]
        k = v["kind"]
        if k in ("int", "real"):
            return 0
        if k == "bool":
            return False
        if k == "enum":
            return m.enum_variants[v["name"]][0]
        if k == "string":
            return ""
        return 0

    return baseline


# Per-model numeric domain, derived from the program's REACHABLE / VISITED set.
# Each entry is name -> (lo, hi). NEVER a hardcoded box: the domain is what the
# dynamics actually touch. For a degenerate-from-init fixed point (van der Pol's
# unstable origin) we widen by iterating off-init probe seeds and taking the
# extent those orbits VISIT — so the grid covers the limit cycle, but only out to
# where the program's own dynamics go, not a fabricated ±3000 plane.
_DOMAIN_CACHE = {}

# Off-init probe DIRECTIONS, expressed as fractions of a search radius. They are
# NOT absolute coordinates: the radius is grown geometrically until an orbit
# escapes the single fixed point, so we discover the attractor's scale from the
# dynamics instead of hardcoding it.
_PROBE_DIRS = [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0),
               (0.7, 0.7), (-0.7, 0.7), (0.15, 0.0)]


def numeric_axes(m):
    return [v for v in m.state_vars if v["kind"] in ("int", "real")]


def _visited_extent_from_probes(m, ax_x, ax_y):
    """Grow off-init probe seeds outward (geometric radius) and follow each
    orbit; return {name: (lo, hi)} of every numeric state var the orbits VISIT.
    Used only when the reachable-from-init set is a single fixed point but the
    program may have a surrounding continuous attractor (van der Pol). Returns {}
    if no probe escapes — i.e. a genuine lone fixed point, route to N/A."""
    init = m.initial_state() or {}
    nums = numeric_axes(m)
    seen_vals = {v["name"]: [] for v in nums}
    escaped = False
    # axis-value-space directions for the two chosen axes; other numeric axes
    # stay at their init (the probe perturbs only the plotted plane).
    dirs = []
    for dx, dy in _PROBE_DIRS:
        dirs.append((dx, dy))
    for radius in (16, 64, 256, 1024, 2867):
        for dx, dy in dirs:
            st = dict(init) if init else {v["name"]: 0 for v in m.state_vars}
            st[ax_x["name"]] = int(round(radius * dx))
            if ax_y is not None and ax_y["kind"] in ("int", "real"):
                st[ax_y["name"]] = int(round(radius * dy))
            cur = st
            local = []
            keyseen = set()
            for _ in range(500):
                nxt = m.successor(cur)
                if nxt is None:
                    break
                k = m._key(nxt)
                if k in keyseen:
                    break
                keyseen.add(k)
                local.append(nxt)
                cur = nxt
            # did this orbit move away from the seed AND not collapse to a point?
            if local:
                for v in nums:
                    vals = [s[v["name"]] for s in local]
                    seen_vals[v["name"]].extend(vals)
                # escape = the orbit visited more than one distinct state
                if len({m._key(s) for s in local}) > 1:
                    escaped = True
        if escaped:
            break
    if not escaped:
        return {}
    out = {}
    for v in nums:
        vals = seen_vals[v["name"]]
        if vals and max(vals) > min(vals):
            out[v["name"]] = (float(min(vals)), float(max(vals)))
    return out


def numeric_domain(m, ax_x, ax_y):
    """name -> (lo, hi) for each numeric state var, derived from the reachable /
    visited set — the honest plotting & seeding domain. Order of preference:
      1. model.axis_bounds(name) — the padded reachable extent (correct whenever
         the reachable set spans a real range: van der Pol's limit cycle when the
         sample walks it, vending balance 0..3, etc.).
      2. probe-visited extent — when the reachable-from-init set is a single fixed
         point, grow off-init orbits and take what they actually VISIT.
    A var with neither (a genuinely lone fixed point on that axis) is absent from
    the dict; callers treat that as 'no meaningful grid here'."""
    key = id(m)
    if key in _DOMAIN_CACHE:
        return _DOMAIN_CACHE[key]
    dom = {}
    nums = numeric_axes(m)
    # Which numeric vars actually VARY in the reachable sample? axis_bounds pads a
    # flat (lo==hi) fixed-point var to ±1, which would masquerade as a real span —
    # so trust it only where the sample has ≥2 distinct values.
    sample = m._sample_states()
    varies = {}
    for v in nums:
        vals = [s[v["name"]] for s in sample if v["name"] in s]
        varies[v["name"]] = len(set(vals)) >= 2
    degenerate = []
    for v in nums:
        b = m.axis_bounds(v["name"])
        if b is not None and varies[v["name"]]:      # a real reachable span
            dom[v["name"]] = b
        else:
            degenerate.append(v)                     # flat in sample (fixed pt)
    if degenerate:                                   # try to find a surrounding
        probed = _visited_extent_from_probes(m, ax_x, ax_y)   # continuous attractor
        for v in degenerate:
            if v["name"] in probed:
                lo, hi = probed[v["name"]]
                pad = (hi - lo) * 0.08
                dom[v["name"]] = (lo - pad, hi + pad)
    _DOMAIN_CACHE[key] = dom
    return dom


def axis_grid(m, v, n, dom):
    """Grid samples + display bounds for one axis variable, scaled to the
    reachable/visited domain `dom` (NEVER a hardcoded box). A numeric axis with no
    domain entry (a lone fixed point) gets a tiny window around its init value —
    there is nothing to grid, and the caller will have routed to N/A anyway."""
    k = v["kind"]
    if k in ("int", "real"):
        b = dom.get(v["name"])
        if b is None:
            init = m.initial_state() or {}
            c = float(init.get(v["name"], 0))
            return [c], (c - 1.0, c + 1.0)
        lo, hi = b
        steps = min(n, int(hi - lo) + 1) if k == "int" else n
        return list(np.linspace(lo, hi, max(2, steps))), (lo, hi)
    if k == "bool":
        return [False, True], (-0.5, 1.5)
    if k == "enum":
        variants = m.enum_variants[v["name"]]
        return list(variants), (-0.5, len(variants) - 0.5)
    return [""], (-0.5, 0.5)
