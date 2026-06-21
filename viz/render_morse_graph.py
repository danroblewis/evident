#!/usr/bin/env python3
"""render_morse_graph.py — Morse graph (recurrence skeleton) for any Evident IR.

Usage:
    python3 viz/render_morse_graph.py <smt2> <schema> <out.png>

The Morse graph is the *condensation DAG* of the reachable transition graph:
one node per strongly-connected component (SCC). Recurrent dynamics live inside
the nontrivial SCCs (cycles); the DAG between them encodes the gradient-like
flow of the system. Each node is classified:

    attractor  — no out-edges in the condensation (flow sinks here)
    repeller   — no in-edges  in the condensation (flow originates here)
    transient  — otherwise (flow passes through)

and marked as a CYCLE (SCC size > 1, recurrent set) vs a single state.

This is computed purely by querying the transition relation through
evident_viz (z3) — never hardcoded:
  * discrete / mixed programs: m.reachable() gives the exact graph.
  * numeric programs: reachable() from the (often fixed-point) initial state
    explores nothing, so we sample successors from a grid of seeds and quantize
    the resulting points into cells, building an approximate flow graph whose
    SCCs reveal the limit cycle / attracting set.
"""
import sys
import os

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__))))
from evident_viz import load

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.patches import FancyBboxPatch
import networkx as nx
from networkx.drawing.nx_pydot import graphviz_layout


# ----------------------------------------------------------------------------
# Build a reachable transition graph (nodes = state keys, edges = transitions)
# ----------------------------------------------------------------------------

def _key(m, state):
    return tuple(state[v["name"]] for v in m.state_vars)


def _abbrev(name):
    """Short tag for a (possibly dotted) state-var name: 'd.has_key' -> 'key',
    'state.balance' -> 'balance', 'state.mode' -> 'mode'."""
    leaf = name.split(".")[-1]
    for pre in ("has_", "is_"):
        if leaf.startswith(pre):
            leaf = leaf[len(pre):]
            break
    return leaf


def _fmt_val(val):
    if val is True:
        return "T"
    if val is False:
        return "F"
    if isinstance(val, float):
        return f"{val:.0f}"
    return str(val)


def _label_for_key(m, key):
    """Label a state by its RANKED vars (top var spelled out, bools as compact
    flags) rather than an anonymous positional tuple. The first ranked var (the
    most expressive, often the enum) gets named prominence; remaining vars become
    a compact key=val strip so the node reads from the axes of meaning, not
    position."""
    vs = m.state_vars
    if not vs:
        return "()"
    parts = [f"{_abbrev(vs[0]['name'])}={_fmt_val(key[0])}"]
    rest = []
    for v, val in zip(vs[1:], key[1:]):
        if v["kind"] == "bool":
            # only surface the True flags — terse and reads as "what's set"
            if val is True:
                rest.append(_abbrev(v["name"]))
        else:
            rest.append(f"{_abbrev(v['name'])}={_fmt_val(val)}")
    head = parts[0]
    if rest:
        return head + "\n[" + " ".join(rest) + "]"
    return head


def build_discrete_graph(m):
    """Exact reachable graph for discrete / mixed (finite-reachable) programs."""
    states, edges = m.reachable()
    G = nx.DiGraph()
    keys = [_key(m, s) for s in states]
    for k in keys:
        G.add_node(k)
    for (i, j) in edges:
        if keys[i] != keys[j]:
            G.add_edge(keys[i], keys[j])
        else:
            # self-loop = a fixed point; keep it so the SCC is nontrivial
            G.add_edge(keys[i], keys[j])
    return G, {k: _label_for_key(m, k) for k in keys}


def build_numeric_orbit_graph(m, steps=400):
    """Exact recurrence graph for a numeric system whose reachable() collapses
    (e.g. the encoded seed is a fixed point) — walk the ACTUAL forward orbit
    from the encoded initial state and quantize coincident points so a limit
    cycle closes into one SCC.

    This never invents off-domain seeds: every node corresponds to a state the
    program REALLY visits from its initial condition. Returns (G, labels) or
    (None, None) when the orbit does not settle (diverges / stays non-recurrent),
    in which case the caller renders an honest N/A card rather than fabricating
    a grid."""
    numeric_vars = [v for v in m.state_vars if v["kind"] in ("int", "real")]
    base = m.initial_state()
    if base is None:
        return None, None

    traj = m.trajectory(start=base, steps=steps)
    if not traj or len(traj) < 1:
        return None, None

    # Robust quantization step per numeric axis from the orbit's OWN spread
    # (not a hardcoded box). One cell ~ 1/40 of the realized range, min 1.
    cell = {}
    for v in numeric_vars:
        vals = [s[v["name"]] for s in traj]
        rng = (max(vals) - min(vals)) if vals else 0.0
        cell[v["name"]] = max(rng / 40.0, 1.0)

    def quant(state):
        parts = []
        for v in m.state_vars:
            val = state[v["name"]]
            if v["kind"] in ("int", "real"):
                parts.append(round(val / cell[v["name"]]))
            else:
                parts.append(val)
        return tuple(parts)

    G = nx.DiGraph()
    rep = {}   # cell-key -> representative real coords (for label)
    prev = None
    for s in traj:
        k = quant(s)
        rep.setdefault(k, tuple(s[v["name"]] for v in numeric_vars))
        G.add_node(k)
        if prev is not None and prev != k:
            G.add_edge(prev, k)
        elif prev == k:
            G.add_edge(k, k)   # fixed point self-loop
        prev = k

    if G.number_of_nodes() == 0:
        return None, None

    labels = {}
    for k, coords in rep.items():
        labels[k] = "(" + ", ".join(f"{c:.0f}" if isinstance(c, float)
                                    else str(c) for c in coords) + ")"
    return G, labels


def draw_na_card(m, out_path, reason):
    """Render an honest 'not meaningful for this program' card instead of a
    fabricated graph."""
    fig, ax = plt.subplots(figsize=(12, 9))
    ax.axis("off")
    ax.text(0.5, 0.62, "N/A — Morse graph not meaningful here",
            ha="center", va="center", fontsize=20, fontweight="bold",
            color="#444444", transform=ax.transAxes)
    ax.text(0.5, 0.46, reason, ha="center", va="center", fontsize=12,
            color="#666666", wrap=True, transform=ax.transAxes)
    fig.suptitle(f"{m.fsm} — Morse graph (condensation of reachable "
                 f"transition graph)", fontsize=15, fontweight="bold", y=0.9)
    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)
    print(f"wrote {out_path}: N/A card ({reason})")


# ----------------------------------------------------------------------------
# Condensation + classification
# ----------------------------------------------------------------------------

def _tint_index(m):
    """Index into the state-key tuple of the variable used to TINT Morse nodes:
    the top-ranked CATEGORICAL var (color is excellent for categorical). Returns
    (index, var) or (None, None) when the model has no categorical var (e.g. a
    pure-numeric system like vanderpol) — then nodes stay white-filled and read
    by role/border alone."""
    cats = m.categorical_vars
    if not cats:
        return None, None
    name = cats[0]["name"]
    names = [v["name"] for v in m.state_vars]
    if name not in names:
        return None, None
    return names.index(name), cats[0]


def condense_and_classify(G, tint_idx=None):
    """Return (C, info) where C is the condensation DAG and info maps each
    condensation node -> dict(role, is_cycle, size, members, label). When
    tint_idx is given, info[n]['dom_cat'] = the dominant value of that categorical
    var among the SCC's member states (the node-fill tint)."""
    sccs = list(nx.strongly_connected_components(G))
    C = nx.condensation(G, scc=sccs)   # node attr 'members' = set of orig nodes

    info = {}
    for cn in C.nodes():
        members = C.nodes[cn]["members"]
        size = len(members)
        # an SCC is recurrent (cycle) if size>1 OR a single node with a self-loop
        is_cycle = size > 1
        if size == 1:
            only = next(iter(members))
            if G.has_edge(only, only):
                is_cycle = True
        indeg = C.in_degree(cn)
        outdeg = C.out_degree(cn)
        if outdeg == 0 and indeg > 0:
            role = "attractor"
        elif indeg == 0 and outdeg > 0:
            role = "repeller"
        elif indeg == 0 and outdeg == 0:
            role = "isolated"
        else:
            role = "transient"
        dom = None
        if tint_idx is not None:
            counts = {}
            for k in members:
                if isinstance(k, tuple) and len(k) > tint_idx:
                    val = k[tint_idx]
                    counts[val] = counts.get(val, 0) + 1
            if counts:
                dom = max(counts, key=counts.get)
        info[cn] = dict(role=role, is_cycle=is_cycle, size=size,
                        members=members, dom_cat=dom)
    return C, info


def simplify_skeleton(C, info, labels):
    """Collapse a large condensation down to its recurrence skeleton for
    legibility. Keep every recurrent (cycle) SCC plus the repeller/attractor
    boundary nodes; merge the cloud of singleton transients into one summary
    node so the limit set stays readable.

    Returns (C2, info2, labels2, note_str)."""
    keep = set()
    for cn in C.nodes():
        i = info[cn]
        if i["is_cycle"] or i["role"] in ("repeller", "attractor", "isolated"):
            keep.add(cn)
    merged = [cn for cn in C.nodes() if cn not in keep]
    if not merged:
        return C, info, labels, ""

    C2 = nx.DiGraph()
    SUMMARY = ("__transient_cloud__",)
    # collect orig members for relabeling
    new_labels = {}
    new_info = {}
    for cn in keep:
        C2.add_node(cn)
        new_info[cn] = info[cn]
        i = info[cn]
        if i["size"] == 1:
            k = next(iter(i["members"]))
            new_labels[cn] = labels.get(k, str(k))
        else:
            new_labels[cn] = None  # node_text will summarize via members

    C2.add_node(SUMMARY)
    new_info[SUMMARY] = dict(role="transient", is_cycle=False,
                             size=len(merged), members={SUMMARY}, dom_cat=None)
    new_labels[SUMMARY] = f"{len(merged)} transient\ncells\n(flow-through)"

    def remap(n):
        return n if n in keep else SUMMARY

    for (u, v) in C.edges():
        a, b = remap(u), remap(v)
        if a != b:
            C2.add_edge(a, b)

    # recompute roles for kept nodes after merge (degrees changed)
    for cn in C2.nodes():
        if cn == SUMMARY:
            continue
        indeg = C2.in_degree(cn)
        outdeg = C2.out_degree(cn)
        i = new_info[cn]
        if outdeg == 0 and indeg > 0:
            role = "attractor"
        elif indeg == 0 and outdeg > 0:
            role = "repeller"
        elif indeg == 0 and outdeg == 0:
            role = "isolated"
        else:
            role = "transient"
        # a recurrent SCC with no outflow is still an attractor (a real attractor)
        new_info[cn] = dict(i, role=role)

    # store explicit-label override on info so node_text can use it
    for cn in C2.nodes():
        if new_labels[cn] is not None:
            new_info[cn] = dict(new_info[cn], _text=new_labels[cn])

    note = (f"  —  skeleton view: {len(keep)} recurrent/boundary SCCs kept, "
            f"{len(merged)} transient SCCs merged")
    return C2, new_info, labels, note


# ----------------------------------------------------------------------------
# Drawing
# ----------------------------------------------------------------------------

ROLE_COLOR = {
    "attractor": "#2e7d32",   # green — flow sinks here
    "repeller":  "#c62828",   # red   — flow sources
    "transient": "#1565c0",   # blue  — pass-through
    "isolated":  "#6a1b9a",   # purple
}


def node_text(info_n, orig_labels):
    if "_text" in info_n:
        return info_n["_text"]
    members = info_n["members"]
    if info_n["size"] == 1:
        k = next(iter(members))
        return orig_labels.get(k, str(k))
    # cycle: show size + a couple of members (now ranked-var labels, not tuples)
    sample = list(members)[:2]
    txt = f"cycle ×{info_n['size']}\n"
    txt += "\n".join(orig_labels.get(k, str(k)) for k in sample)
    if info_n["size"] > 2:
        txt += "\n…"
    return txt


def _build_tint_map(C, info, tint_var):
    """Map each distinct dominant-categorical value present in the graph to a
    light fill color (the COLOR channel, excellent for categorical). Returns
    {value: hexcolor}. Empty when there's no tint var."""
    if tint_var is None:
        return {}
    vals = []
    for cn in C.nodes():
        d = info[cn].get("dom_cat")
        if d is not None and d not in vals:
            vals.append(d)
    if not vals:
        return {}
    import matplotlib.colors as mcolors
    base = plt.get_cmap("tab10")
    out = {}
    for i, v in enumerate(vals):
        r, g, b, _ = base(i % 10)
        # lighten toward white so the role-colored border + black text stay legible
        out[v] = mcolors.to_hex((0.55 + 0.45 * r, 0.55 + 0.45 * g, 0.55 + 0.45 * b))
    return out


def draw(C, info, orig_labels, fsm, viz_type, out_path, subtitle="",
         tint_var=None):
    fig, ax = plt.subplots(figsize=(12, 9))
    tint_map = _build_tint_map(C, info, tint_var)

    if C.number_of_nodes() == 0:
        ax.text(0.5, 0.5, "empty graph", ha="center", va="center",
                fontsize=18, transform=ax.transAxes)
        ax.axis("off")
        fig.suptitle(f"{fsm} — {viz_type}", fontsize=16, fontweight="bold")
        fig.savefig(out_path, dpi=120, bbox_inches="tight")
        plt.close(fig)
        return

    # layout the condensation DAG top-down with graphviz dot
    try:
        pos = graphviz_layout(C, prog="dot")
    except Exception:
        pos = nx.spring_layout(C, seed=7)

    # normalize positions into axes coords for manual boxed drawing
    xs = [p[0] for p in pos.values()]
    ys = [p[1] for p in pos.values()]
    minx, maxx = min(xs), max(xs)
    miny, maxy = min(ys), max(ys)
    spanx = max(maxx - minx, 1)
    spany = max(maxy - miny, 1)

    def norm(p):
        # center a single node (and any degenerate axis) instead of pinning to a
        # corner; spread multi-node layouts across the [0,1] box.
        nx_ = 0.5 if (maxx - minx) < 1e-9 else (p[0] - minx) / spanx
        ny_ = 0.5 if (maxy - miny) < 1e-9 else (p[1] - miny) / spany
        return (nx_, ny_)

    npos = {n: norm(p) for n, p in pos.items()}

    # edges first
    for (u, v) in C.edges():
        x0, y0 = npos[u]
        x1, y1 = npos[v]
        ax.annotate("", xy=(x1, y1), xytext=(x0, y0),
                    arrowprops=dict(arrowstyle="-|>", color="#888888",
                                    lw=1.4, shrinkA=22, shrinkB=22,
                                    connectionstyle="arc3,rad=0.05"))

    # nodes as boxes
    for n in C.nodes():
        x, y = npos[n]
        i = info[n]
        color = ROLE_COLOR[i["role"]]
        txt = node_text(i, orig_labels)
        # cycle nodes get a thicker, double border
        lw = 3.0 if i["is_cycle"] else 1.6
        box = FancyBboxPatch((x - 0.001, y - 0.001), 0.002, 0.002,
                             boxstyle="round,pad=0.5",
                             linewidth=0, facecolor="none")
        ax.add_patch(box)
        fc = tint_map.get(i.get("dom_cat"), "white")
        bbox = dict(boxstyle="round,pad=0.35",
                    fc=fc, ec=color, lw=lw)
        ax.text(x, y, txt, ha="center", va="center", fontsize=8.5,
                color="#111111", bbox=bbox, zorder=5)
        # role marker dot above the box
        ax.scatter([x], [y], s=0)  # keep autoscale honest

    ax.set_xlim(-0.08, 1.08)
    ax.set_ylim(-0.08, 1.08)
    ax.axis("off")

    # legend
    from matplotlib.lines import Line2D
    legend_elems = [
        Line2D([0], [0], marker="s", color="w", label=role,
               markerfacecolor="white", markeredgecolor=col, markeredgewidth=2,
               markersize=12)
        for role, col in ROLE_COLOR.items()
    ]
    legend_elems.append(
        Line2D([0], [0], marker="s", color="w",
               label="cycle SCC (thick border)",
               markerfacecolor="white", markeredgecolor="#333333",
               markeredgewidth=3, markersize=12))
    role_legend = ax.legend(handles=legend_elems, loc="lower center",
                            bbox_to_anchor=(0.5, -0.06), ncol=5,
                            frameon=False, fontsize=9)
    ax.add_artist(role_legend)

    # secondary legend: node FILL = dominant value of the top categorical var
    # (the added COLOR channel — keeps the border=role encoding intact).
    if tint_map:
        from matplotlib.patches import Patch
        tag = _abbrev(tint_var["name"]) if tint_var else "category"
        tint_elems = [
            Patch(facecolor=col, edgecolor="#666666", label=f"{tag}={_fmt_val(val)}")
            for val, col in tint_map.items()
        ]
        # outside the axes (right margin) so it never overlaps a DAG node, which
        # can sit anywhere in the [0,1] layout box.
        ax.legend(handles=tint_elems, loc="upper left",
                  bbox_to_anchor=(1.01, 1.0),
                  title=f"node fill:\ndominant {tag}", title_fontsize=9,
                  ncol=1, frameon=True, fontsize=8.5, framealpha=0.95)

    title = f"{fsm} — Morse graph (condensation of reachable transition graph)"
    fig.suptitle(title, fontsize=15, fontweight="bold", y=0.98)
    if subtitle:
        ax.set_title(subtitle, fontsize=10, color="#444444", pad=14)

    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)


# ----------------------------------------------------------------------------

def main():
    if len(sys.argv) != 4:
        print("usage: render_morse_graph.py <smt2> <schema> <out.png>",
              file=sys.stderr)
        sys.exit(2)
    smt2, schema, out = sys.argv[1], sys.argv[2], sys.argv[3]
    m = load(smt2, schema)

    if m.is_discrete():
        G, labels = build_discrete_graph(m)
        sub = (f"{G.number_of_nodes()} reachable states, "
               f"{G.number_of_edges()} transitions  (exact)")
    else:
        numeric_vars = [v for v in m.state_vars
                        if v["kind"] in ("int", "real")]
        if numeric_vars:
            # mixed or pure numeric: use the EXACT reachable set (finite mixed
            # systems like vending/wc terminate). If reachable() collapses to a
            # single state, the encoded seed sits at a fixed point — walk the
            # real forward orbit (which may close into a limit cycle). We NEVER
            # invent off-domain grid seeds: every plotted node is a state the
            # program really visits.
            try:
                states, edges = m.reachable(limit=4000)
            except Exception:
                states, edges = [], []
            if len(states) > 1 and len(states) < 2000:
                G = nx.DiGraph()
                keys = [_key(m, s) for s in states]
                lab = {}
                for s, k in zip(states, keys):
                    G.add_node(k)
                    lab[k] = _label_for_key(m, k)
                for (i, j) in edges:
                    G.add_edge(keys[i], keys[j])
                labels = lab
                sub = (f"{G.number_of_nodes()} reachable states, "
                       f"{G.number_of_edges()} transitions  (exact, mixed)")
            elif len(states) == 1:
                # reachable() found a single state — either a true fixed point
                # or the explorer can't unfold a continuous orbit. Try the real
                # forward orbit from the seed; only that, never a fabricated grid.
                G, labels = build_numeric_orbit_graph(m)
                if G is None or G.number_of_nodes() <= 1:
                    # genuine fixed point: plot the one real reachable state.
                    G = nx.DiGraph()
                    keys = [_key(m, s) for s in states]
                    G.add_node(keys[0])
                    G.add_edge(keys[0], keys[0])
                    labels = {keys[0]: _label_for_key(m, keys[0])}
                    sub = ("1 reachable state — the encoded seed is a fixed "
                           "point (no further dynamics)  (exact)")
                else:
                    sub = (f"{G.number_of_nodes()} states on the real forward "
                           f"orbit from the initial condition  (exact orbit)")
            else:
                draw_na_card(m, out,
                             "the reachable transition set could not be "
                             "enumerated for this numeric program — a Morse "
                             "graph would require fabricating off-domain states.")
                return
        else:
            G, labels = build_discrete_graph(m)
            sub = f"{G.number_of_nodes()} reachable states"

    tint_idx, tint_var = _tint_index(m)
    C, info = condense_and_classify(G, tint_idx=tint_idx)
    if C.number_of_nodes() > 24:
        C, info, labels, note = simplify_skeleton(C, info, labels)
        sub = sub + note
    if tint_var is not None:
        sub = sub + f"  ·  fill = dominant {_abbrev(tint_var['name'])}"
    draw(C, info, labels, m.fsm, "morse_graph", out, subtitle=sub,
         tint_var=tint_var)
    print(f"wrote {out}: {C.number_of_nodes()} SCC nodes, "
          f"{C.number_of_edges()} condensation edges")


if __name__ == "__main__":
    main()
