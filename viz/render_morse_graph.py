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


def build_numeric_graph(m, span=3200, n=13, fp_scale=None):
    """Approximate flow graph for a numeric system.

    Sample a grid of seed points over [-span, span] in every int/real var,
    step each one forward once, and quantize both endpoints onto a coarse cell
    lattice. Edges between cells form a flow graph whose recurrent SCCs trace
    the attracting set (e.g. a limit cycle)."""
    numeric_vars = [v for v in m.state_vars if v["kind"] in ("int", "real")]
    other_vars = [v for v in m.state_vars if v["kind"] not in ("int", "real")]

    # Quantization cell size: ~ 2*span / cells_per_axis. Coarse on purpose — the
    # Morse graph is about the RECURRENCE SKELETON, not a fine flow field, so we
    # want the limit set to collapse into one (or few) nontrivial SCC.
    cells = 8
    cell = (2.0 * span) / cells

    def quant(state):
        parts = []
        for v in m.state_vars:
            val = state[v["name"]]
            if v["kind"] in ("int", "real"):
                parts.append(round(val / cell))
            else:
                parts.append(val)
        return tuple(parts)

    # Grid of seeds over the numeric axes (other vars seeded from initial state).
    base = m.initial_state() or {}
    axis = [(-span) + (2 * span) * i / (n - 1) for i in range(n)]

    import itertools
    seeds = []
    grids = [axis for _ in numeric_vars]
    for combo in itertools.product(*grids):
        s = dict(base)
        for v, val in zip(numeric_vars, combo):
            s[v["name"]] = int(round(val)) if v["kind"] == "int" else val
        # ensure non-numeric vars have *some* value
        for v in other_vars:
            if v["name"] not in s:
                if v["kind"] == "bool":
                    s[v["name"]] = False
                elif v["kind"] == "enum":
                    s[v["name"]] = m.enum_variants[v["name"]][0]
        seeds.append(s)

    G = nx.DiGraph()
    centroids = {}   # cell -> representative numeric coords (for layout/label)

    def record_centroid(cell_key, state):
        coords = tuple(state[v["name"]] for v in numeric_vars)
        centroids.setdefault(cell_key, coords)

    for s in seeds:
        nxt = m.successor(s)
        if nxt is None:
            continue
        a = quant(s)
        b = quant(nxt)
        record_centroid(a, s)
        record_centroid(b, nxt)
        G.add_node(a)
        G.add_node(b)
        G.add_edge(a, b)
        # follow the chain a few steps so the limit set closes into a cycle
        cur = nxt
        for _ in range(40):
            nx2 = m.successor(cur)
            if nx2 is None:
                break
            ca = quant(cur)
            cb = quant(nx2)
            record_centroid(ca, cur)
            record_centroid(cb, nx2)
            G.add_node(ca)
            G.add_node(cb)
            G.add_edge(ca, cb)
            cur = nx2

    labels = {}
    for ck, coords in centroids.items():
        labels[ck] = "(" + ", ".join(f"{int(round(c))}" for c in coords) + ")"
    return G, labels


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
        return ((p[0] - minx) / spanx, (p[1] - miny) / spany)

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
            # mixed or pure numeric: try exact reachable first (finite mixed
            # systems like vending terminate); fall back to grid sampling if it
            # collapses to a trivial graph.
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
            else:
                G, labels = build_numeric_graph(m)
                sub = ("grid-sampled flow on a quantized lattice  "
                       f"({G.number_of_nodes()} cells, "
                       f"{G.number_of_edges()} edges) — approximate")
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
