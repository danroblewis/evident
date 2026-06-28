"""transition_matrix_data.py — the ABSTRACT `<out>.data.json` substrate for render_transition_matrix.

The PNG draws the state×state adjacency heatmap; this captures the matrix's MEANING as data: its
shape (n states), the mode (all-initial-conditions / exact-reachable / sampled grid), the total
number of transitions, and the per-row out-degree distribution. For a random walk the expert
expects each state to map to up to 9 successors (the king-move stencil), so a matrix whose every
row has out-degree 1 (a sampled grid that binned a single successor per state) is a caught
regression — the nondeterministic fan collapsed.

Schema (`<out>.data.json`):

    {
      "view":         "transition_matrix",
      "model":        "<fsm name>",
      "mode":         "<all initial conditions / exact reachable / sampled state grid>",
      "n_states":     int,
      "n_transitions": int,                 # total lit cells (matrix sum)
      "max_out_degree": int,                # the widest row — the branching actually captured
      "out_degree_hist": {deg: count}       # how many rows have each out-degree
    }

Built from the SAME adjacency matrix the renderer draws. The branching the transition CAN produce
(vs what the matrix captured) is checked independently by the golden test via a one-step successor
probe (tests/viz_golden/successors.py).
"""
import json
from collections import Counter


def build(model, mat, mode):
    """Assemble the dict from the adjacency matrix `mat` (a 2-D numpy 0/1 array) + the `mode`
    string the renderer chose."""
    rowsums = [int(r) for r in mat.sum(axis=1)] if mat is not None and mat.size else []
    hist = {str(k): v for k, v in sorted(Counter(rowsums).items())}
    return {
        "view": "transition_matrix",
        "model": model.fsm,
        "mode": mode,
        "n_states": int(mat.shape[0]) if mat is not None and mat.size else 0,
        "n_transitions": int(mat.sum()) if mat is not None and mat.size else 0,
        "max_out_degree": max(rowsums) if rowsums else 0,
        "out_degree_hist": hist,
    }


def write(out_path, data):
    """Write `<out>.data.json`. Mirrors region_data.write: never raises — a sidecar failure must
    not fail the render."""
    try:
        with open(out_path + ".data.json", "w") as f:
            json.dump(data, f, indent=2)
    except Exception:
        pass
