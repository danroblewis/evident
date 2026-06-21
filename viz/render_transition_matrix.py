#!/usr/bin/env python3
"""render_transition_matrix — adjacency-matrix heatmap for any Evident IR.

CLI:  python3 viz/render_transition_matrix.py <smt2> <schema> <out_path>

The transition relation state = f(_state) is, abstractly, a directed graph on
states. This renderer orders a representative set of states and draws the
adjacency MATRIX as a heatmap: cell (row i, col j) is lit iff there is a
transition from state i to state j (queried from z3, never hardcoded).

  * DISCRETE (bool/enum/string only) — exact reachable state set, ordered, and
    the full adjacency matrix with per-state labels on both axes.
  * NUMERIC / MIXED — we can't enumerate an infinite state space, so we sample a
    representative grid of states (numeric axes gridded, discrete axes swept over
    their variants), query each one's successor(s), and bin the resulting
    next-states back onto the same sampled set. The matrix then shows the
    coarse-grained flow structure (limit cycles show as off-diagonal bands).

Channel mapping (Cleveland-McGill / Mackinlay):
  * The two MATRIX AXES (row order = column order) carry the full state — this is
    POSITION, the strongest channel. We don't pick two vars; the matrix shows the
    whole transition relation. What we DO choose is the ORDERING of states along
    that shared axis: states are sorted so the TOP CATEGORICAL var (var_class
    'cat' — enum/bool, e.g. d.room, state.mode) forms contiguous blocks. A
    block-diagonal-ish structure then means "transitions stay within a mode";
    off-block bands mean "mode switches".
  * That same top categorical var is then ENCODED ON COLOR via a side ribbon on
    both axes (the honest secondary use of hue for a categorical) AND by coloring
    the per-state tick labels. So the blocks of same-mode states are readable at a
    glance, while the matrix cells themselves stay a neutral transition heatmap.
  * Purely-numeric programs (no categorical var — e.g. vanderpol) have no ribbon;
    states are ordered by the primary numeric axis so the limit-cycle flow reads
    as an off-diagonal band, and a coarse magnitude gradient ribbon labels the
    ordering.

Always emits exactly one PNG (dpi=120). Degrades to a titled placeholder only if
no states can be obtained at all.
"""
import sys
import os

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np

from evident_viz import load


# --------------------------------------------------------------------------- #
# State-set construction
# --------------------------------------------------------------------------- #
def discrete_states_and_edges(m):
    """Exact reachable graph for a purely discrete program."""
    states, edges = m.reachable()
    return states, edges


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


def _categorical_palette(values):
    """Map distinct categorical values -> RGBA colours (stable, qualitative)."""
    seen = []
    for val in values:
        if val not in seen:
            seen.append(val)
    cmap = plt.get_cmap("tab10" if len(seen) <= 10 else "tab20")
    lut = {val: cmap(i % cmap.N) for i, val in enumerate(seen)}
    return lut, seen


def _ribbon_colours(ribbon_var, ribbon_values, m):
    """Per-state ribbon RGBA + legend descriptor. Categorical -> qualitative LUT;
    numeric fallback -> magnitude gradient. Returns (label_colours, legend)."""
    is_cat = m.var_class(ribbon_var) == "cat"
    if is_cat:
        lut, distinct = _categorical_palette(ribbon_values)
        label_colours = [lut[v] for v in ribbon_values]
        legend = ("cat", ribbon_var["name"], lut, distinct)
    else:
        vals = np.array([float(v) for v in ribbon_values], dtype=float)
        lo, hi = float(vals.min()), float(vals.max())
        norm = (vals - lo) / (hi - lo) if hi > lo else np.zeros_like(vals)
        grad = plt.get_cmap("plasma")(norm)
        label_colours = [tuple(c) for c in grad]
        legend = ("num", ribbon_var["name"], (lo, hi), None)
    return label_colours, legend


def _ribbon_legend(m, ribbon_var, ribbon_values):
    return _ribbon_colours(ribbon_var, ribbon_values, m)


def draw_ribbons(fig, ax, ribbon_var, ribbon_values, m):
    """Two colour ribbons (one left of the rows, one above the columns) encoding
    the top categorical var, so same-value blocks are visible. For the numeric
    fallback the ribbon is a magnitude gradient. Returns label_colours, where
    label_colours[i] is the colour for tick i."""
    n = len(ribbon_values)
    label_colours, _ = _ribbon_colours(ribbon_var, ribbon_values, m)
    row_rgba = np.array([label_colours])                    # 1 x n x 4
    col_rgba = np.array([[c] for c in label_colours])       # n x 1 x 4

    bbox = ax.get_position()
    pad = 0.012
    w = 0.018
    # Left ribbon (rows / "from" state).
    ax_left = fig.add_axes([bbox.x0 - w - pad, bbox.y0, w, bbox.height])
    ax_left.imshow(col_rgba, aspect="auto", interpolation="nearest",
                   extent=[0, 1, n - 0.5, -0.5])
    ax_left.set_xticks([]); ax_left.set_yticks([])
    ax_left.set_ylabel(ribbon_var["name"], fontsize=8)
    # Top ribbon (cols / "to" state).
    ax_top = fig.add_axes([bbox.x0, bbox.y1 + pad, bbox.width, w])
    ax_top.imshow(row_rgba, aspect="auto", interpolation="nearest",
                  extent=[-0.5, n - 0.5, 0, 1])
    ax_top.set_xticks([]); ax_top.set_yticks([])

    return label_colours


def draw_legend(fig, ax, legend):
    if legend is None:
        return
    kind, name, payload, distinct = legend
    if kind == "cat":
        lut = payload
        from matplotlib.patches import Patch
        handles = [Patch(facecolor=lut[v], edgecolor="0.3",
                         label=str(v)) for v in distinct]
        ax.legend(handles=handles, title=name, fontsize=7, title_fontsize=8,
                  loc="upper left", bbox_to_anchor=(1.18, 1.0),
                  framealpha=0.9, borderpad=0.6)
    else:
        lo, hi = payload
        from matplotlib.cm import ScalarMappable
        from matplotlib.colors import Normalize
        sm = ScalarMappable(norm=Normalize(lo, hi), cmap="plasma")
        sm.set_array([])
        cb = fig.colorbar(sm, ax=ax, fraction=0.025, pad=0.10, shrink=0.55)
        cb.set_label(f"{name} (order key)", fontsize=8)


# --------------------------------------------------------------------------- #
# Rendering
# --------------------------------------------------------------------------- #
def render(m, out_path):
    discrete = m.is_discrete()
    try:
        if discrete:
            states, edges = discrete_states_and_edges(m)
            # Order states meaningfully (cluster by top categorical), then
            # remap the exact edge index-pairs through the permutation.
            ordered, ribbon_var, ribbon_values = order_states(m, states)
            pos = {m._key(s): i for i, s in enumerate(states)}
            perm = [pos[m._key(s)] for s in ordered]          # ordered[i] = states[perm[i]]
            inv = {old: new for new, old in enumerate(perm)}
            states = ordered
            edges = {(inv[i], inv[j]) for (i, j) in edges}
            mat = build_matrix(m, states, edges=edges)
            mode = "exact reachable graph"
        else:
            # Finer grid when the state is purely numeric (the matrix is the
            # only place the flow structure shows); coarser when discrete axes
            # already multiply the count up.
            disc = any(v["kind"] in ("bool", "enum") for v in m.state_vars)
            states = sample_states(m, num_grid=5 if disc else 9)
            states, ribbon_var, ribbon_values = order_states(m, states)
            mat = build_matrix(m, states, edges=None)
            mode = "sampled state grid"
    except Exception as e:  # noqa: BLE001
        placeholder(m, out_path, f"could not build state set: {e}")
        return

    if not states:
        placeholder(m, out_path, "no states (initial_state unavailable)")
        return

    labels = [m.label(s) for s in states]
    n = len(states)

    # Figure size scales with N so labels stay readable.
    side = max(6.0, min(0.32 * n + 2.5, 22.0))
    fig, ax = plt.subplots(figsize=(side, side))

    im = ax.imshow(mat, cmap="viridis", vmin=0, vmax=1, aspect="equal",
                   interpolation="nearest")

    ax.set_xticks(range(n))
    ax.set_yticks(range(n))
    fs = max(4, min(9, int(420 / max(n, 1))))
    ax.set_xticklabels(labels, rotation=90, fontsize=fs, family="monospace")
    ax.set_yticklabels(labels, fontsize=fs, family="monospace")
    ax.set_xlabel("to  state", fontsize=11)
    ax.set_ylabel("from  state", fontsize=11)

    # Light grid between cells for readability.
    ax.set_xticks(np.arange(-0.5, n, 1), minor=True)
    ax.set_yticks(np.arange(-0.5, n, 1), minor=True)
    ax.grid(which="minor", color="white", linewidth=0.4, alpha=0.3)
    ax.tick_params(which="minor", length=0)

    nnz = int(mat.sum())
    order_note = (f"ordered by {ribbon_var['name']}" if ribbon_var is not None
                  else "unordered")
    ax.set_title(
        f"{m.fsm}  ·  transition_matrix\n"
        f"{mode} — {n} states, {nnz} transitions  ·  {order_note}",
        fontsize=13, pad=14,
    )
    cbar = fig.colorbar(im, ax=ax, fraction=0.046, pad=0.04,
                        ticks=[0, 1], shrink=0.6)
    cbar.ax.set_yticklabels(["no", "yes"])
    cbar.set_label("transition exists", fontsize=9)

    # Colour channel: legend (categorical patches or a numeric gradient bar) goes
    # in first because it can shift the main axes; then settle the layout; THEN
    # place the absolute-positioned side ribbons against the final axes box, and
    # tint the tick labels. No tight_layout after the ribbons (it would orphan
    # them).
    if ribbon_var is not None and ribbon_values is not None:
        _, legend = _ribbon_legend(m, ribbon_var, ribbon_values)
        draw_legend(fig, ax, legend)

    fig.tight_layout()
    fig.canvas.draw()

    if ribbon_var is not None and ribbon_values is not None:
        label_colours = draw_ribbons(fig, ax, ribbon_var, ribbon_values, m)
        for tl, c in zip(ax.get_xticklabels(), label_colours):
            tl.set_color(c)
        for tl, c in zip(ax.get_yticklabels(), label_colours):
            tl.set_color(c)

    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)


def placeholder(m, out_path, reason):
    fig, ax = plt.subplots(figsize=(8, 6))
    ax.axis("off")
    kinds = ", ".join(sorted({v["kind"] for v in m.state_vars}))
    ax.set_title(f"{m.fsm}  ·  transition_matrix", fontsize=14)
    ax.text(0.5, 0.5, f"N/A for {kinds} state:\n{reason}",
            ha="center", va="center", fontsize=13,
            bbox=dict(boxstyle="round", fc="#fff0f0", ec="#cc4444"))
    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)


def main(argv):
    if len(argv) != 4:
        print("usage: render_transition_matrix.py <smt2> <schema> <out_path>",
              file=sys.stderr)
        return 2
    smt2, schema, out = argv[1], argv[2], argv[3]
    m = load(smt2, schema)
    os.makedirs(os.path.dirname(os.path.abspath(out)), exist_ok=True)
    render(m, out)
    print(f"wrote {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
