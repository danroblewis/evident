#!/usr/bin/env python3
"""transition_matrix_build.py — the DATA layer for render_transition_matrix.py.

Builds the finite state set + N×N adjacency matrix + meaningful state ordering
from the model, with no plotting policy. The renderer imports `sample_states`,
`build_matrix`, and `order_states`; the rest are their internal helpers.
"""
import numpy as np


def sample_states(m, num_grid=7):
    """A representative finite state set for numeric / mixed programs.

    Numeric axes are gridded over an inferred range; discrete axes sweep their
    full variant set. We then close the set forward by one step so that the
    successors land somewhere we can index (binning to nearest sampled state).
    """
    # Build per-axis sample value lists.
    axis_values = []
    for v in m.state_vars:
        k = v["kind"]
        if k == "bool":
            axis_values.append((v, [False, True]))
        elif k == "enum":
            axis_values.append((v, list(m.enum_variants[v["name"]])))
        elif k in ("int", "real"):
            lo, hi = infer_numeric_range(m, v)
            grid = np.linspace(lo, hi, num_grid)
            if k == "int":
                grid = sorted(set(int(round(x)) for x in grid))
            else:
                grid = list(grid)
            axis_values.append((v, grid))
        else:  # string and anything else: single placeholder
            axis_values.append((v, [""]))

    # Cartesian product of axes -> candidate states.
    states = [{}]
    for v, vals in axis_values:
        states = [dict(s, **{v["name"]: val}) for s in states for val in vals]
    # Cap to keep the matrix legible / cheap.
    if len(states) > 64:
        idx = np.linspace(0, len(states) - 1, 64).astype(int)
        states = [states[i] for i in sorted(set(idx))]
    return states


def numeric_axes(m):
    return [v for v in m.state_vars if v["kind"] in ("int", "real")]


def infer_numeric_range(m, v):
    """Guess a sampling range for a numeric axis purely by querying the
    transition. We can't trust the initial state alone — it may be a fixed point
    (e.g. an origin equilibrium), so probing from it never moves. Instead we cast
    a coarse net of off-axis seeds across a wide default window, follow each one
    forward, and read off the magnitude the orbit actually visits."""
    seen = []
    n_axes = numeric_axes(m)

    # Seed points: the initial state, plus a spread of off-origin probes so we
    # discover the operating magnitude even when the origin is an equilibrium.
    seeds = []
    init = m.initial_state()
    if init is not None:
        seeds.append(dict(init))
    base = {}
    for v2 in m.state_vars:
        k = v2["kind"]
        if k == "bool":
            base[v2["name"]] = False
        elif k == "enum":
            base[v2["name"]] = m.enum_variants[v2["name"]][0]
        elif k == "string":
            base[v2["name"]] = ""
        else:
            base[v2["name"]] = 0
    span = 3200.0
    for axis in n_axes:
        for mult in (-1.0, -0.4, 0.4, 1.0):
            sp = dict(base)
            sp[axis["name"]] = int(round(span * mult)) if axis["kind"] == "int" \
                else span * mult
            seeds.append(sp)

    for seed in seeds:
        cur = dict(seed)
        for _ in range(60):
            val = cur.get(v["name"])
            if isinstance(val, (int, float)):
                seen.append(val)
            nxt = m.successor(cur)
            if nxt is None:
                break
            cur = nxt

    if seen:
        mag = max(abs(min(seen)), abs(max(seen)))
        if mag > 1:
            return -mag * 1.15, mag * 1.15
    return -span, span


def nearest_index(state, states, m):
    """Index of the sampled state closest to `state` (euclidean over numeric
    axes; exact match required on discrete axes)."""
    best, best_d = None, None
    for i, s in enumerate(states):
        d = 0.0
        ok = True
        for v in m.state_vars:
            a, b = state.get(v["name"]), s[v["name"]]
            if v["kind"] in ("int", "real"):
                d += (float(a) - float(b)) ** 2
            else:
                if a != b:
                    ok = False
                    break
        if not ok:
            continue
        if best_d is None or d < best_d:
            best, best_d = i, d
    return best


def build_matrix(m, states, edges=None):
    """N x N adjacency matrix. If `edges` (exact, for discrete) is given use it;
    otherwise query successors of each sampled state and bin to nearest."""
    n = len(states)
    mat = np.zeros((n, n), dtype=float)
    if edges is not None:
        for (i, j) in edges:
            mat[i, j] = 1.0
        return mat
    for i, s in enumerate(states):
        for nxt in m.successors(s, limit=16):
            j = nearest_index(nxt, states, m)
            if j is not None:
                mat[i, j] = 1.0
    return mat


# --------------------------------------------------------------------------- #
# Meaningful state ordering + the categorical colour channel
# --------------------------------------------------------------------------- #
def order_states(m, states):
    """Sort states so the TOP CATEGORICAL var forms contiguous blocks (its values
    cluster on the shared row/col axis). Secondary sort keys keep within-block
    order stable and readable: remaining categoricals, then numerics ascending.

    Returns (ordered_states, ribbon_var, ribbon_values) where ribbon_var is the
    categorical chosen for the colour ribbon (None if the program is purely
    numeric), and ribbon_values is the per-ordered-state value of that var."""
    cats = m.categorical_vars
    nums = m.numeric_vars

    if cats:
        ribbon_var = cats[0]
        # Order the categorical's values: enums by their declared variant order,
        # bools False<True, strings lexicographically.
        rib_name = ribbon_var["name"]
        if ribbon_var["kind"] == "enum":
            variant_rank = {v: i for i, v in
                            enumerate(m.enum_variants.get(rib_name, []))}
            cat_key = lambda val: variant_rank.get(val, 999)
        else:
            cat_key = lambda val: (val if not isinstance(val, bool) else int(val))

        def sort_key(s):
            primary = cat_key(s[rib_name])
            secondary = []
            for v in cats[1:]:
                val = s[v["name"]]
                secondary.append(int(val) if isinstance(val, bool) else str(val))
            for v in nums:
                secondary.append(float(s[v["name"]]))
            return (primary, *secondary)

        ordered = sorted(states, key=sort_key)
        ribbon_values = [s[rib_name] for s in ordered]
        return ordered, ribbon_var, ribbon_values

    # Purely numeric: order by the primary numeric axis (then the rest), so the
    # flow reads as a band. Ribbon encodes the primary axis as a magnitude gradient.
    if nums:
        prim = nums[0]
        ordered = sorted(states,
                         key=lambda s: tuple(float(s[v["name"]]) for v in nums))
        ribbon_values = [s[prim["name"]] for s in ordered]
        return ordered, prim, ribbon_values

    return list(states), None, None
