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
# Rendering
# --------------------------------------------------------------------------- #
def render(m, out_path):
    discrete = m.is_discrete()
    try:
        if discrete:
            states, edges = discrete_states_and_edges(m)
            mat = build_matrix(m, states, edges=edges)
            mode = "exact reachable graph"
        else:
            # Finer grid when the state is purely numeric (the matrix is the
            # only place the flow structure shows); coarser when discrete axes
            # already multiply the count up.
            disc = any(v["kind"] in ("bool", "enum") for v in m.state_vars)
            states = sample_states(m, num_grid=5 if disc else 9)
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
    ax.set_title(
        f"{m.fsm}  ·  transition_matrix\n"
        f"{mode} — {n} states, {nnz} transitions",
        fontsize=13, pad=14,
    )
    cbar = fig.colorbar(im, ax=ax, fraction=0.046, pad=0.04,
                        ticks=[0, 1], shrink=0.6)
    cbar.ax.set_yticklabels(["no", "yes"])
    cbar.set_label("transition exists", fontsize=9)

    fig.tight_layout()
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
