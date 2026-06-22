#!/usr/bin/env python3
"""basin_support.py — shared primitives for render_basin_map.py.

Path-agnostic helpers used by BOTH the discrete-graph basin path and the
numeric-field basin path: the qualitative palette, the N/A placeholder card,
axis/facet/ordinal projection via the channel API, Tarjan SCC condensation,
and the phase-invariant attractor-signature clustering. No plotting policy
lives here beyond the placeholder card; the two render paths stay in
render_basin_map.py.
"""
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402

from overlay_points import write_points  # noqa: E402


# A qualitative palette big enough for the terminal sets we see.
PALETTE = [
    "#4477AA", "#EE6677", "#228833", "#CCBB44", "#66CCEE",
    "#AA3377", "#BBBBBB", "#FF8C00", "#117733", "#882255",
    "#44AA99", "#999933", "#332288", "#DDCC77", "#CC6677",
]


def _placeholder(out_path, fsm, reason):
    fig, ax = plt.subplots(figsize=(8, 6))
    ax.axis("off")
    ax.text(0.5, 0.58, "N/A for basin_map", ha="center", va="center",
            fontsize=20, weight="bold", color="#aa3333")
    ax.text(0.5, 0.44, reason, ha="center", va="center", fontsize=12,
            color="#333333", wrap=True)
    ax.set_title(f"{fsm} — basin_map", fontsize=14, weight="bold")
    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)
    write_points(out_path, [])           # N/A card → no hoverable points


# --------------------------------------------------------------------------
# axis / facet selection via the channel-mapping API
# --------------------------------------------------------------------------
def _choose_axes(m):
    """Return up to two state-var dicts for the x,y projection, via the channel
    API. The basin grid wants NUMERIC axes when available (position is the
    top-ranked channel and decodes quantitative best), so we prefer numeric_vars
    and fall back to assign_channels for the remaining slots (enum/bool ordinals).
    """
    numeric = m.numeric_vars
    chans = m.assign_channels(["x", "y"])
    axes = []
    # fill from numeric first (best for a continuous seed grid)
    for v in numeric:
        if v not in axes:
            axes.append(v)
        if len(axes) == 2:
            return axes
    # top up from the channel assignment (categorical ordinals as a last resort)
    for ch in ("x", "y"):
        v = chans[ch]
        if v is not None and v not in axes:
            axes.append(v)
        if len(axes) == 2:
            break
    return axes


def _choose_facet(m, axes, states):
    """Pick the SUITABLE facet variable via the shared faceting guard
    (m.facet_var): a low-cardinality categorical that stays ~constant within a
    run. Faceting by a var that CHANGES along the trajectory (e.g. vending's
    state.mode on the limit cycle) splits the dynamics across panels — the guard
    rejects those. Must not already be an axis. Returns (var, values) or
    (None, None).

    The returned `values` are ONLY those the facet var actually TAKES across the
    reachable `states` — a declared enum variant that no reachable state visits
    (e.g. find's state.s5 ∈ {Unseen, Pending, Visited} but every reachable state
    is Unseen) would otherwise render a permanently-empty panel. And if fewer
    than 2 distinct values occur, the var is constant: it carries zero faceting
    information, so DON'T facet at all."""
    axis_names = {a["name"] for a in axes}

    def values_of(v):
        if v["kind"] == "enum":
            return m.enum_variants[v["name"]]
        if v["kind"] == "bool":
            return [False, True]
        return None

    v = m.facet_var()
    if v is None or v["name"] in axis_names:
        return None, None
    vals = values_of(v)
    if vals is None:
        return None, None
    # Keep only declared values that some reachable state actually visits, in the
    # declared order. Suppresses empty panels (and rejects a constant facet var).
    present = {st[v["name"]] for st in states if v["name"] in st}
    vals = [x for x in vals if x in present]
    if len(vals) < 2:
        return None, None
    return v, vals


def _ordinal(m, var, value):
    """Project a state value onto a real number for plotting."""
    k = var["kind"]
    if k in ("int", "real"):
        return float(value)
    if k == "bool":
        return 1.0 if value else 0.0
    if k == "enum":
        variants = m.enum_variants[var["name"]]
        return float(variants.index(value))
    if k == "string":
        return 0.0
    return 0.0


def _axis_label(var):
    return f"{var['name']}  [{var['kind']}]"



# --------------------------------------------------------------------------
# graph condensation: SCCs -> terminal basins (used by both paths)
# --------------------------------------------------------------------------
def _tarjan_scc(n, adj):
    """Return list of SCCs (each a list of node ids), via iterative Tarjan."""
    index = [None] * n
    low = [0] * n
    on_stack = [False] * n
    stack = []
    sccs = []
    counter = [0]

    for root in range(n):
        if index[root] is not None:
            continue
        work = [(root, 0)]
        while work:
            v, pi = work[-1]
            if pi == 0:
                index[v] = low[v] = counter[0]
                counter[0] += 1
                stack.append(v)
                on_stack[v] = True
            recursed = False
            neighbors = adj[v]
            while pi < len(neighbors):
                w = neighbors[pi]
                pi += 1
                if index[w] is None:
                    work[-1] = (v, pi)
                    work.append((w, 0))
                    recursed = True
                    break
                elif on_stack[w]:
                    low[v] = min(low[v], index[w])
            if recursed:
                continue
            work[-1] = (v, pi)
            # post-process v: update parent low later; first relax children lows
            for w in neighbors:
                if on_stack[w]:
                    low[v] = min(low[v], low[w])
            if low[v] == index[v]:
                comp = []
                while True:
                    w = stack.pop()
                    on_stack[w] = False
                    comp.append(w)
                    if w == v:
                        break
                sccs.append(comp)
            work.pop()
            if work:
                pv = work[-1][0]
                low[pv] = min(low[pv], low[v])
    return sccs


def _attractor_signature(m, cycle):
    """A PHASE-INVARIANT signature for an attractor (cycle of states). Two
    trajectories that land on the same limit cycle must get the same signature
    regardless of where on the cycle they happened to stop — otherwise every
    phase reads as its own 'basin'. So for numeric axes we use the cycle's
    geometric centroid and its mean radius about that centroid (both phase
    invariant), NOT the raw per-state means. Discrete axes (enum/bool) use the
    set of values visited, since those genuinely distinguish attractors."""
    if not cycle:
        return ("num", 0.0, 0.0, 0.0)
    num_vars = [v for v in m.state_vars if v["kind"] in ("int", "real")]
    disc_vars = [v for v in m.state_vars if v["kind"] in ("enum", "bool",
                                                          "string")]
    sig = []
    # numeric: centroid magnitude + mean radius (size of the orbit)
    for v in num_vars:
        vals = [_ordinal(m, v, st[v["name"]]) for st in cycle]
        sig.append(sum(vals) / len(vals))          # centroid coord
    if num_vars:
        cx = [sum(_ordinal(m, v, st[v["name"]]) for st in cycle) / len(cycle)
              for v in num_vars]
        radii = []
        for st in cycle:
            d = sum((_ordinal(m, v, st[v["name"]]) - cx[i]) ** 2
                    for i, v in enumerate(num_vars)) ** 0.5
            radii.append(d)
        sig.append(sum(radii) / len(radii))        # mean orbit radius
    # discrete: the SET of visited values, projected to an ordinal multiset key
    for v in disc_vars:
        visited = sorted(set(round(_ordinal(m, v, st[v["name"]]))
                             for st in cycle))
        # encode the visited-set as separated coords (one big number per set)
        sig.append(float(sum(val * (1000 ** i)
                             for i, val in enumerate(visited))))
    sig.append(float(len(cycle)))
    return tuple(sig)


def _cluster(sigs, tol=400.0):
    """Greedy clustering of signature vectors. Numeric coords use absolute tol;
    this is enough to separate a limit cycle from a fixed point and distinct
    discrete attractors. Returns (labels, centers)."""
    centers = []
    labels = []
    for s in sigs:
        best = -1
        bestd = None
        for i, c in enumerate(centers):
            d = _sig_dist(s, c)
            if bestd is None or d < bestd:
                bestd = d
                best = i
        if best >= 0 and bestd <= tol:
            labels.append(best)
            # online mean update
            c = centers[best]
            centers[best] = tuple((a + b) / 2 for a, b in zip(c, s))
        else:
            centers.append(s)
            labels.append(len(centers) - 1)
    return labels, centers


def _sig_dist(a, b):
    n = min(len(a), len(b))
    return sum(abs(a[i] - b[i]) for i in range(n))


def _sig_layout(m):
    """Index map into a signature vector (see _attractor_signature):
      [ num centroid coords (one per numeric var, in state-var order),
        mean orbit radius (only if any numeric var),
        discrete set-codes (one per enum/bool/string var),
        cycle length ]"""
    num_vars = [v for v in m.state_vars if v["kind"] in ("int", "real")]
    disc_vars = [v for v in m.state_vars if v["kind"] in ("enum", "bool",
                                                          "string")]
    num_idx = {v["name"]: i for i, v in enumerate(num_vars)}
    base = len(num_vars)
    radius_idx = base if num_vars else None
    disc_start = base + (1 if num_vars else 0)
    disc_idx = {v["name"]: disc_start + i for i, v in enumerate(disc_vars)}
    return num_idx, radius_idx, disc_idx, num_vars, disc_vars
