#!/usr/bin/env python3
"""Shared value→axis projection for the viz renderers.

Every renderer that plots a state var on a numeric axis needs the same mapping:
int/real → the value itself, bool → 0/1, enum → its variant index. That core was
copy-pasted across render_scatter_matrix, render_basin_map (basin_support),
render_fixedpoint_map (fixedpoint_attractors), and render_occupancy_heatmap
(occupancy_collect) — byte-identical in those four branches.

What is NOT shared, and deliberately stays at the call site, is the STRING /
fallback policy: some views map an unseen string to 0.0, others to a hash bucket,
the timing/time-series views return a (coord, label) pair instead. Those are
genuine per-view choices (see concern #407), so `ordinal_core` covers ONLY the
unambiguous int/real/bool/enum branches and returns None for everything else —
each caller keeps its own string/None handling. This dedups the identical 90%
without flattening the divergent 10%.
"""


def ordinal_core(m, var, value):
    """Project an int/real/bool/enum state value onto a float axis coordinate.

    Returns None for string / unknown kinds — the caller supplies its own policy
    for those (they differ per view, intentionally; see module docstring)."""
    k = var["kind"]
    if k in ("int", "real"):
        return float(value)
    if k == "bool":
        return 1.0 if value else 0.0
    if k == "enum":
        return float(m.enum_variants[var["name"]].index(value))
    return None
