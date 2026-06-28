#!/usr/bin/env python3
"""render_transition_matrix — adjacency-matrix heatmap for any Evident IR.

CLI:  python3 viz/render_transition_matrix.py <smt2> <schema> <out_path>

The transition relation state = f(_state) is, abstractly, a directed graph on
states. This renderer orders a representative set of states and draws the
adjacency MATRIX as a heatmap: cell (row i, col j) is lit iff there is a
transition from state i to state j (queried from z3, never hardcoded).

  * ALL INITIAL CONDITIONS — for a finitely-discrete program the matrix rows/cols
    are EVERY valid carried-state assignment (Model.full_state_graph, ignoring
    is_first_tick), and the cells come from the real transition edges. cell (i,j)
    lit ⇔ state_i → state_j. This is the honest state×state incidence over the
    WHOLE space — the same global root set basin_map / state_graph use — NOT one
    z3 model's from-init orbit (which for a deterministic FSM is a single chain).
  * EXACT REACHABLE GRAPH (from init) — when the program isn't discretely
    enumerable but its from-init BFS still CLOSES below a cap (a terminating
    mixed/numeric chain whose state carries Ints — a clock 0..4, a cursor — but
    whose reachable graph is a short chain, NOT a continuous space). The seed
    orbit is all we can enumerate without gridding a continuous axis.
  * NUMERIC / MIXED (unbounded or continuous) — when no real-transition state set
    closes (the BFS hits the cap, or the seed is a lone fixed point), we can't
    enumerate an infinite state space, so we sample a representative grid of
    states (numeric axes gridded, discrete axes swept over their variants), query
    each one's successor(s), and bin the resulting next-states back onto the same
    sampled set. The matrix then shows the coarse-grained flow structure (limit
    cycles show as off-diagonal bands).

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
import transition_matrix_data
# State-set construction + adjacency-matrix building live in the data module.
from transition_matrix_build import sample_states, build_matrix, order_states


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
    # Left ribbon (rows / "from" state). Sits LEFT of the y-axis ID tick labels
    # (S0…S12) — a larger left gap so the ribbon doesn't overprint them.
    left_pad = 0.055
    ax_left = fig.add_axes([bbox.x0 - w - left_pad, bbox.y0, w, bbox.height])
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
def _select_root(m):
    """Choose the matrix's root state set + cells, strongest first:
      1. ALL INITIAL CONDITIONS — for a finitely-discrete program the rows/cols are
         EVERY valid carried-state assignment (full_state_graph) and the cells come
         from the real transition edges: the honest state×state incidence over the
         WHOLE space, not one z3 model's orbit. The same global root basin_map /
         state_graph use. (full_state_graph's own discrete flag decides
         enumerability — NOT m.is_discrete(), so a bounded-int carry like counter
         0..5 / traffic's timer 0..2 takes the global root, not the from-init orbit.)
      2. exact reachable graph (from init) — not discretely enumerable but the
         from-init BFS closes below a cap (a terminating mixed/numeric chain).
      3. sampled state grid — genuinely unbounded / continuous: grid the numeric
         axes, sweep discrete ones, bin successors.
    Returns (states, mat, ribbon_var, ribbon_values, mode).

    Cases 1 and 2 share `_exact`: both have a REAL edge set, so we order the states
    and remap the edge index-pairs through the ordering permutation, then fill the
    matrix from those edges (no fabricated grid)."""
    def _exact(states, edges, mode):
        ordered, rv, rvals = order_states(m, states)
        pos = {m._key(s): i for i, s in enumerate(states)}
        perm = [pos[m._key(s)] for s in ordered]          # ordered[i] = states[perm[i]]
        inv = {old: new for new, old in enumerate(perm)}
        edges = {(inv[i], inv[j]) for (i, j) in edges}
        return ordered, build_matrix(m, ordered, edges=edges), rv, rvals, mode

    FINITE_CAP = 200
    g_states, g_edges, info = m.full_state_graph(limit=5000)
    if info["discrete"] and not info["capped"] and 2 <= len(g_states):
        return _exact(g_states, g_edges, "all initial conditions")
    try:
        exact_states, exact_edges = m.reachable(limit=FINITE_CAP)
    except Exception:  # noqa: BLE001
        exact_states, exact_edges = [], []
    if 2 <= len(exact_states) < FINITE_CAP:
        return _exact(exact_states, exact_edges, "exact reachable graph (from init)")
    # Genuinely unbounded / continuous: no real-transition state set closes, so grid
    # the numeric axes (coarser when discrete axes already multiply the count up) and
    # bin successors onto the sampled set.
    disc = any(v["kind"] in ("bool", "enum") for v in m.state_vars)
    states = sample_states(m, num_grid=5 if disc else 9)
    states, ribbon_var, ribbon_values = order_states(m, states)
    return states, build_matrix(m, states, edges=None), \
        ribbon_var, ribbon_values, "sampled state grid"


def _build_states_matrix(m, out_path):
    """Build the ordered state set + adjacency matrix (via _select_root), subsampled
    to a legible size. Returns (states, mat, ribbon_var, ribbon_values, mode,
    sampled_note, total_states) or None if it already emitted a placeholder."""
    try:
        states, mat, ribbon_var, ribbon_values, mode = _select_root(m)
    except Exception as e:  # noqa: BLE001
        placeholder(m, out_path, f"could not build state set: {e}")
        return None

    if not states:
        placeholder(m, out_path, "no states (initial_state unavailable)")
        return None

    total_states = n = len(states)
    sampled_note = ""

    # Above ~30 states the matrix cells themselves shrink past readability and
    # the ID legend becomes a wall of text. Representatively subsample (evenly
    # over the meaningful ordering, so the categorical blocks survive) and say so.
    MAX_MATRIX_STATES = 30
    if n > MAX_MATRIX_STATES:
        keep = sorted(set(np.linspace(0, n - 1, MAX_MATRIX_STATES).astype(int)))
        states = [states[i] for i in keep]
        if ribbon_values is not None:
            ribbon_values = [ribbon_values[i] for i in keep]
        mat = mat[np.ix_(keep, keep)]
        n = len(states)
        sampled_note = f"  ·  showing {n} of {total_states} states"

    return states, mat, ribbon_var, ribbon_values, mode, sampled_note, total_states


def _draw_matrix_grid(ax, mat, ids, n):
    """Paint the matrix heatmap + the S0/S1… axis ticks + the cell grid. Returns
    the AxesImage so the caller can attach a colorbar."""
    im = ax.imshow(mat, cmap="viridis", vmin=0, vmax=1, aspect="equal",
                   interpolation="nearest")

    ax.set_xticks(range(n))
    ax.set_yticks(range(n))
    fs = max(5, min(10, int(520 / max(n, 1))))
    ax.set_xticklabels(ids, rotation=90, fontsize=fs, family="monospace")
    ax.set_yticklabels(ids, fontsize=fs, family="monospace")
    ax.set_xlabel("to  state", fontsize=11)
    ax.set_ylabel("from  state", fontsize=11)

    # Light grid between cells for readability.
    ax.set_xticks(np.arange(-0.5, n, 1), minor=True)
    ax.set_yticks(np.arange(-0.5, n, 1), minor=True)
    ax.grid(which="minor", color="white", linewidth=0.4, alpha=0.3)
    ax.tick_params(which="minor", length=0)
    return im


def render(m, out_path):
    built = _build_states_matrix(m, out_path)
    if built is None:
        # _build_states_matrix already emitted a placeholder PNG; still drop an HONEST data
        # sidecar so the golden suite sees "no matrix" rather than a missing file.
        transition_matrix_data.write(out_path, transition_matrix_data.build(m, None, "n/a"))
        return
    states, mat, ribbon_var, ribbon_values, mode, sampled_note, _total = built
    n = len(states)

    # ABSTRACT substrate (golden suite): the matrix's structure — shape, mode, transition count,
    # and per-row out-degree (the branching the matrix captured). Built from the SAME `mat` drawn
    # below, so the data and the picture agree. Mirrors the PNG; never fails the render.
    transition_matrix_data.write(out_path, transition_matrix_data.build(m, mat, mode))

    # A matrix whose axes are the full state-tuples overprints into illegible
    # text the moment the tuple is more than a couple of fields (brackets,
    # toposort: 8-12 carried leaves each). So we NEVER tick the axes with the
    # tuples. Instead each state gets a short ID (S0, S1, …) on the axis, and a
    # compact side LEGEND maps IDs -> tuple values. The matrix stays the focus;
    # the values are one glance away without crushing the labels.
    ids = [f"S{i}" for i in range(n)]

    # Figure size scales with N so cells/IDs stay readable.
    side = max(6.0, min(0.32 * n + 2.5, 18.0))
    fig, ax = plt.subplots(figsize=(side, side))

    im = _draw_matrix_grid(ax, mat, ids, n)

    nnz = int(mat.sum())
    order_note = (f"ordered by {ribbon_var['name']}" if ribbon_var is not None
                  else "unordered")
    ax.set_title(
        f"{m.fsm}  ·  transition_matrix\n"
        f"{mode} — {n} states, {nnz} transitions  ·  {order_note}{sampled_note}",
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

    # ID -> value legend last, against the settled axes box (after tight_layout
    # and the ribbons, so it doesn't get reflowed on top of them).
    draw_id_legend(fig, ax, ids, states, m)

    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)


def draw_id_legend(fig, ax, ids, states, m):
    """Compact side panel mapping each short state ID (S0, S1, …) to its full
    tuple value. The axes carry only the IDs (so they stay legible no matter how
    many fields the state has); this is where the reader recovers what each ID
    means. Laid out as a monospace block to the FAR right of the figure (clear of
    the colorbar / categorical ribbon legend), in figure coordinates."""
    field_names = [v["name"] for v in m.interface_vars]
    lines = [f"{i} = {m.label(s)}" for i, s in zip(ids, states)]

    # Split into columns so a tall legend (up to ~30 rows) doesn't run off the
    # bottom: ~16 rows per column, columns laid out left-to-right.
    per_col = 16
    n_cols = (len(lines) + per_col - 1) // per_col
    cols = [lines[c * per_col:(c + 1) * per_col] for c in range(n_cols)]

    # Place at a fixed far-right figure x (past the colorbar + ribbon legend) and
    # GROW the figure to the right to fit, so nothing overlaps and nothing clips.
    x0 = 1.02                       # just past the right edge of the current fig
    fig.subplots_adjust()           # ensure positions are current
    col_dx = 0.16                   # figure-fraction width per legend column

    # Header: the tuple field order. Wrap it so a 12-field name list doesn't run
    # off — break into lines of a few fields each.
    per_line = 3
    hdr_lines = ["(" if field_names else "()"]
    for k in range(0, len(field_names), per_line):
        chunk = ", ".join(field_names[k:k + per_line])
        sep = "," if k + per_line < len(field_names) else ")"
        hdr_lines.append("  " + chunk + sep)

    fig.text(x0, 0.97, "state IDs", fontsize=10, fontweight="bold",
             family="monospace", va="top", ha="left")
    fig.text(x0, 0.94, "\n".join(hdr_lines), fontsize=7, color="0.35",
             family="monospace", va="top", ha="left")
    y_body = 0.94 - 0.020 * (len(hdr_lines) + 1)
    for ci, col in enumerate(cols):
        fig.text(x0 + ci * col_dx, y_body, "\n".join(col),
                 fontsize=7.5, family="monospace", va="top", ha="left")


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
