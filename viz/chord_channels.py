#!/usr/bin/env python3
"""chord_channels.py — channel mapping + flow gathering for the chord diagram.

The analysis half of `render_chord_diagram.py`: it decides, from a model alone (no
matplotlib), WHICH variable rides each visual channel and gathers the transition flow
the drawing half then renders. Two stages:

  - CHANNEL MAPPING (Cleveland-McGill / Mackinlay): `pick_primary` chooses the NODE
    axis (the categorical with the most observed classes, or a binned numeric var);
    `pick_color_var` chooses a SECOND categorical for the ARC HUE — but only if its
    per-arc majority projection actually varies (else the color channel is dropped).
  - FLOW GATHERING: `gather_flow` walks the reachable edge set (categorical) or the
    real orbit (numeric, binned over its reachable extent) into a
    {(src_label, dst_label): count} dict plus the per-arc color category.

Everything here reads `m` (the model) and returns plain data; nothing draws. The
drawing half (`render_chord_diagram.py`) imports `pick_primary`, `pick_color_var`,
`gather_flow`, `orbit_states`, `_observed_cardinality` from here.
"""
import numpy as np


# --- channel mapping: node var (categorical position) + arc-color var ----------

MIN_NODE_CLASSES = 3   # a chord with <3 nodes degenerates to 2 dots / 1 arc


def _observed_cardinality(m, name):
    """Number of DISTINCT values a var actually takes over the REACHABLE set
    (not the schema's declared variant count). A bool that latches false->true,
    or an enum the program only ever sits in one state of, is degenerate as a
    node axis even though its declared cardinality is >1."""
    try:
        states, _ = m.reachable()
    except Exception:
        states = m.trajectory(steps=200)
    return len({st.get(name) for st in states if name in st})


def pick_primary(m):
    """NODE channel = the categorical var with the HIGHEST OBSERVED cardinality
    over the reachable set (NOT categorical_vars[0] in schema order — that can be
    a latching bool that collapses the whole picture to 2 nodes / 1 arc). Return
    (var_dict, node_labels, project_fn, mode_str).

    Selection / degeneracy guard:
      - Among categoricals, rank by how many distinct values they actually take.
      - Require >= MIN_NODE_CLASSES distinct node classes; a 2-value bool (or an
        enum stuck in <3 states) makes a degenerate chord and is rejected here.
      - If no categorical clears the bar, fall back to binning the top numeric
        var (a real banded flow). If there's no numeric var either, return None
        for the var and let draw() route to an honest N/A card."""
    cats = m.categorical_vars

    # rank categoricals by OBSERVED cardinality (highest first), break ties toward
    # enums (named labels read better than true/false) then shorter names.
    ranked = sorted(
        ((v, _observed_cardinality(m, v["name"])) for v in cats),
        key=lambda vc: (-vc[1], 0 if vc[0]["kind"] == "enum" else 1, len(vc[0]["name"])),
    )

    if ranked and ranked[0][1] >= MIN_NODE_CLASSES:
        v = ranked[0][0]
        if v["kind"] == "enum":
            labels = list(m.enum_variants[v["name"]])
            return v, labels, (lambda st: st[v["name"]]), "enum"
        if v["kind"] == "bool":
            labels = ["false", "true"]
            return v, labels, (lambda st: "true" if st[v["name"]] else "false"), "bool"
        # string: labels discovered dynamically from observed values
        return v, None, (lambda st: st[v["name"]]), "string"

    # no categorical with enough distinct node classes — bin the top numeric var
    # into bands over its REAL reachable extent (still honest, non-fabricated).
    if m.numeric_vars:
        v = m.numeric_vars[0]
        return v, None, None, "numeric"   # binning resolved after we know range

    # nothing to put on the node axis at all — caller renders the N/A card.
    return None, None, None, "none"


def _color_proj(v):
    """A (var_dict, labels, project_fn) triple for a candidate color var."""
    name = v["name"]
    if v["kind"] == "enum":
        labels = list(m_enum_variants_safe(v))
        return v, labels, (lambda st, n=name: st[n])
    if v["kind"] == "bool":
        return v, ["false", "true"], (lambda st, n=name: "true" if st[n] else "false")
    return v, None, (lambda st, n=name: st[n])   # string: dynamic labels


def m_enum_variants_safe(v):
    # set lazily by pick_color_var's caller; kept here to avoid a closure over m
    return _COLOR_ENUM_VARIANTS.get(v["name"], [])


_COLOR_ENUM_VARIANTS = {}


def pick_color_var(m, node_var, proj):
    """COLOR channel = a SECOND categorical var (not the node var) whose value, as
    PROJECTED ONTO THE ARCS WE ACTUALLY DRAW, genuinely VARIES. Returns
    (var_dict, color_labels, project_fn) or None.

    The subtlety this guards (the dungeon round-3 defect): a bool can vary across
    the raw reachable states yet still be MONOCHROME on the chord, because many
    full-states collapse onto one (src_room -> dst_room) arc and the per-arc
    MAJORITY vote washes the variation out. d.has_key / d.has_torch / d.escaped all
    do this — the room you land in fixes the flag, so every arc's majority is the
    same value and the color channel carries zero information. We therefore rank
    candidate color vars by how much their per-ARC majority projection varies
    (Gini-style: 1 means a perfect split, 0 means monochrome) and pick the most
    varying one. If NO candidate's arc projection varies, return None so draw()
    drops the color channel entirely rather than rendering it monochrome."""
    _COLOR_ENUM_VARIANTS.clear()
    for v in m.carried:
        if v["kind"] == "enum":
            _COLOR_ENUM_VARIANTS[v["name"]] = list(m.enum_variants.get(v["name"], []))

    # the exact node->node arcs we will draw, as (src_label, dst_label) -> [dst states]
    arc_dst_states = _arc_destination_states(m, node_var, proj)
    if not arc_dst_states:
        return None

    best = None
    best_var = -1.0
    for v in m.categorical_vars:
        if v["name"] == node_var["name"]:
            continue
        cand = _color_proj(v)
        cproj = cand[2]
        # per-arc majority label, then how varied those majorities are across arcs
        majorities = []
        for dst_states in arc_dst_states.values():
            votes = {}
            for st in dst_states:
                lab = cproj(st)
                votes[lab] = votes.get(lab, 0) + 1
            majorities.append(max(votes, key=votes.get))
        variation = _label_variation(majorities)
        if variation > best_var:
            best_var, best = variation, cand

    # require the chosen var to ACTUALLY vary across arcs; monochrome => no color.
    if best is None or best_var <= 0.0:
        return None
    return best


def _arc_destination_states(m, node_var, proj):
    """{(src_label, dst_label): [destination full-states]} for the reachable edge
    set, projected onto the node var. This is exactly the arc set draw() renders,
    so color-variance measured here reflects what the eye actually sees."""
    try:
        states, edges = m.reachable()
    except Exception:
        return {}
    arcs = {}
    for (i, j) in edges:
        key = (proj(states[i]), proj(states[j]))
        arcs.setdefault(key, []).append(states[j])
    return arcs


def _label_variation(labels):
    """1 - sum(p_k^2) over the label distribution (Gini impurity): 0 when all arcs
    share one majority label (monochrome — useless color channel), approaching 1
    as the arcs split evenly across labels."""
    if not labels:
        return 0.0
    counts = {}
    for lab in labels:
        counts[lab] = counts.get(lab, 0) + 1
    n = len(labels)
    return 1.0 - sum((c / n) ** 2 for c in counts.values())


# --- gather transition flow as a dict {(src_label, dst_label): count} ----------

def gather_flow(m, var, labels, proj, mode, color):
    """Returns (labels, flow, numrange, arc_cat, color_labels) where flow maps
    (src,dst)->count and arc_cat maps (src,dst)->the majority color-var category
    of the destination (None when no color var)."""
    flow = {}
    cat_votes = {}            # (src,dst) -> {color_label: count}
    nbins = 8
    cproj = color[2] if color else None

    def bump(a, b, dst_state=None):
        flow[(a, b)] = flow.get((a, b), 0) + 1
        if cproj is not None and dst_state is not None:
            d = cat_votes.setdefault((a, b), {})
            lab = cproj(dst_state)
            d[lab] = d.get(lab, 0) + 1

    def finish_cat():
        return {k: max(v, key=v.get) for k, v in cat_votes.items()}

    if mode in ("enum", "bool", "string"):
        states, edges = m.reachable()
        if mode == "string":
            seen = []
            for st in states:
                lab = proj(st)
                if lab not in seen:
                    seen.append(lab)
            labels = seen if seen else ["(none)"]
        # resolve dynamic string color labels from observed dst values
        clabels = color[1] if color else None
        if color is not None and clabels is None:
            seen = []
            for st in states:
                lab = cproj(st)
                if lab not in seen:
                    seen.append(lab)
            clabels = seen
        for (i, j) in edges:
            bump(proj(states[i]), proj(states[j]), states[j])
        return labels, flow, None, finish_cat(), clabels

    # numeric: bin the primary var over its REACHABLE extent, and chord between
    # the consecutive states of the ACTUAL orbit (never a fabricated grid sweep
    # over a guessed ±3000 box). orbit() returns the real visited successor chain.
    orbit = orbit_states(m, var)
    lo, hi = orbit_extent(orbit, var["name"])
    if lo is None or (hi - lo) <= 1:
        # reachable set is a single point / degenerate — nothing to chord.
        return [], flow, None, {}, None
    edges_bin = np.linspace(lo, hi, nbins + 1)
    centers = (edges_bin[:-1] + edges_bin[1:]) / 2.0
    labels = [bin_label(centers[k]) for k in range(nbins)]

    def to_bin(val):
        k = int(np.clip(np.searchsorted(edges_bin, val, side="right") - 1, 0, nbins - 1))
        return labels[k]

    for cur, nxt in zip(orbit, orbit[1:]):
        bump(to_bin(cur[var["name"]]), to_bin(nxt[var["name"]]), nxt)

    return labels, flow, (lo, hi), finish_cat(), (color[1] if color else None)


def orbit_states(m, var):
    """The ACTUAL visited states for the numeric node var, as a successor chain.
    Prefers the orbit from the initial state; if that is a degenerate single point
    (an unstable fixed point at the seed — vanderpol's origin), probe a few
    off-origin seeds and keep the richest real orbit. Every state here comes from
    m.successor / m.trajectory — never a hardcoded box."""
    best = m.trajectory(steps=400)
    if _spread(best, var["name"]) > 1:
        return best
    # degenerate from the default init: the attractor may live off the seed.
    # Probe a handful of small off-origin seeds (still REAL dynamics) to recover
    # a limit-cycle / spiral orbit. We do NOT widen to a guessed plotting box;
    # these seeds just kick the system off an unstable fixed point.
    nums = m.numeric_vars
    base = m.initial_state() or {}
    for kick in (16, 64, 256, 1024):
        seed = dict(base)
        if nums:
            seed[nums[0]["name"]] = (base.get(nums[0]["name"], 0) or 0) + kick
        cand = m.trajectory(start=seed, steps=400)
        if _spread(cand, var["name"]) > _spread(best, var["name"]):
            best = cand
    return best


def _spread(states, name):
    vals = [s[name] for s in states if name in s]
    return (max(vals) - min(vals)) if vals else 0


def orbit_extent(orbit, name):
    """(lo, hi) of the node var over the ACTUAL orbit, padded slightly. None when
    the orbit is empty/degenerate — the caller then routes to an honest N/A."""
    vals = [s[name] for s in orbit if name in s]
    if not vals:
        return None, None
    lo, hi = float(min(vals)), float(max(vals))
    if hi - lo <= 1:
        return lo, hi
    pad = (hi - lo) * 0.02
    return lo - pad, hi + pad


def bin_label(c):
    if abs(c) >= 1000:
        return f"{c/1000:+.1f}k"
    return f"{c:+.0f}"
