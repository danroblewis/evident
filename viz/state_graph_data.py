"""state_graph_data.py — the ABSTRACT `<out>.data.json` substrate for render_state_graph.

The PNG draws the reachable state-transition graph; this captures the graph's MEANING as
machine-checkable data: how many states, the per-node out-degree distribution, the branching
factor, the mode the renderer fell into (exact graph / global / trajectory / seeded), and whether
the graph degenerated to a trivial run. A golden test asserts the graph has the structure the
model's transition relation actually has (e.g. a random walk's nodes branch up to 9 ways), so a
regression that collapses the fan to a single self-looping node is caught as data, not pixels.

Schema (`<out>.data.json`):

    {
      "view":        "state_graph",
      "model":       "<fsm name>",
      "mode":        "<which graph was drawn: exact / global / trajectory / seeded ...>",
      "n_nodes":     int,
      "n_edges":     int,
      "max_out_degree":  int,        # the branching factor actually drawn (a fan's width)
      "out_degree_hist": {deg: count},  # how many nodes have each out-degree
      "degenerate":  bool,           # True iff the graph collapsed to ≤1 node (a non-graph)
      "init_state":  {"<short>": v}|null
    }

Built from the SAME networkx graph the renderer draws (so the data can never disagree with the
picture). The branching the model's transition CAN produce (vs what was drawn) is checked
independently by the golden test via a one-step successor probe (tests/viz_golden/successors.py).
"""
import json
from collections import Counter

from render_common import short


def build(model, G, states, mode):
    """Assemble the dict from the drawn graph G (a networkx DiGraph), its `states` map, and the
    `mode` string the renderer chose. G/states may be None (nothing drawn → a degenerate record)."""
    init = model.initial_state() or {}
    init_state = ({short(k): v for k, v in init.items()} or None)
    if G is None or G.number_of_nodes() == 0:
        return {"view": "state_graph", "model": model.fsm, "mode": mode or "none",
                "n_nodes": 0, "n_edges": 0, "max_out_degree": 0, "out_degree_hist": {},
                "degenerate": True, "init_state": init_state}
    degs = [d for _, d in G.out_degree()]
    hist = {str(k): v for k, v in sorted(Counter(degs).items())}
    n = G.number_of_nodes()
    return {
        "view": "state_graph",
        "model": model.fsm,
        "mode": mode or "unknown",
        "n_nodes": n,
        "n_edges": G.number_of_edges(),
        "max_out_degree": max(degs) if degs else 0,
        "out_degree_hist": hist,
        "degenerate": n <= 1,
        "init_state": init_state,
    }


def write(out_path, data):
    """Write `<out>.data.json`. Mirrors region_data.write: never raises — a sidecar failure must
    not fail the render."""
    try:
        with open(out_path + ".data.json", "w") as f:
            json.dump(data, f, indent=2)
    except Exception:
        pass
