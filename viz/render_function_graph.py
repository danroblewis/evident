#!/usr/bin/env python3
"""render_function_graph.py — the functionizer's COMPILED DATA-FLOW DAG.

Diagram 1 of the functionizer family. The functionizer splits the transition relation into per-output
FUNCTIONS (functionize.extract_functions); each function reads other variables. This draws that as a
graph: a node per carried variable (+ the is_first_tick driver), an edge W→V when V's next value is
computed from W's previous value, and a self-loop when V reads its own previous value (a recurrence).

What it reveals that the dynamics views can't: the COUPLING STRUCTURE. A 2-cycle (pos↔vel) IS an
oscillation — mutual feedback. A pure DAG (only self-loops, no cross-cycle) is a driven pipeline.
Nodes are typed: a DEPENDENT var (functionized — Scalar or Guarded) vs an INDEPENDENT driver. The
function each node computes is printed under its name, so the picture is also the program's update law.
"""
import sys

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import matplotlib.patches as mpatches
import networkx as nx

sys.path.insert(0, "viz")
from evident_viz import load
from functionize import extract_functions, function_summary

DEP_C = "#1f77b4"      # dependent (functionized)
DRV_C = "#7d8590"      # independent driver
CYC_C = "#d62728"      # an edge on a feedback cycle


def _summary(step):
    if step["kind"] == "scalar":
        return step["expr"][:34]
    return f"piecewise · {len(step['branches'])} branches"


def _draw_edges(g, pos, on_cycle, ax):
    """Cross-edges (cycle edges red), then self-loops as a ↻ glyph (networkx hides them under nodes)."""
    cross = [(u, v) for (u, v) in g.edges if u != v]
    nx.draw_networkx_edges(g, pos, edgelist=[e for e in cross if e not in on_cycle],
                           edge_color="#566", width=1.6, arrowsize=18, ax=ax, connectionstyle="arc3,rad=0.12")
    if any(e in on_cycle for e in cross):
        nx.draw_networkx_edges(g, pos, edgelist=[e for e in cross if e in on_cycle],
                               edge_color=CYC_C, width=2.6, arrowsize=20, ax=ax, connectionstyle="arc3,rad=0.12")
    for (u, _v) in [(u, v) for (u, v) in g.edges if u == v]:
        x, y = pos[u]
        ax.annotate("↻", (x, y + 0.10), fontsize=15, ha="center", color="#566")


def render(smt2, schema, out_path):
    m = load(smt2, schema)
    f = extract_functions(m)
    prev_to_var = {v["prev"]: v["name"] for v in m.carried if v.get("prev")}
    step_by_var = {s["var"]: s for s in f["steps"]}

    g = nx.DiGraph()
    for s in f["steps"]:
        g.add_node(s["var"], dependent=True)
    # edges: a dep `_W` (prev of W) means this var reads W's previous value → W feeds V.
    drivers = set()
    for s in f["steps"]:
        deps = sorted({d for b in s.get("branches", []) for d in b["deps"]} | set(s.get("deps", [])))
        for d in deps:
            src = prev_to_var.get(d)
            if src is None:                          # a non-carried driver (e.g. is_first_tick)
                if d not in step_by_var:
                    g.add_node(d, dependent=False); drivers.add(d)
                src = d
            g.add_edge(src, s["var"])

    if g.number_of_nodes() == 0:
        _placeholder(out_path, m.fsm, "no functionized variables to graph")
        return

    # edges that lie on a directed cycle (feedback) — the oscillation signal.
    on_cycle = set()
    for cyc in nx.simple_cycles(g):
        if len(cyc) >= 2:                            # a real mutual loop, not a bare self-recurrence
            for i in range(len(cyc)):
                on_cycle.add((cyc[i], cyc[(i + 1) % len(cyc)]))

    pos = nx.spring_layout(g, seed=1, k=1.6)
    fig, ax = plt.subplots(figsize=(8.5, 7.0))
    node_colors = [DEP_C if g.nodes[n].get("dependent") else DRV_C for n in g.nodes]
    nx.draw_networkx_nodes(g, pos, node_color=node_colors, node_size=2200, ax=ax,
                           edgecolors="#0b0f14", linewidths=1.5)
    _draw_edges(g, pos, on_cycle, ax)

    # var NAME inside the node (short); the function summary BELOW it so it never clips the circle (#308).
    nx.draw_networkx_labels(g, pos, {n: n for n in g.nodes}, font_size=9, font_color="#fff", ax=ax)
    for n in g.nodes:
        if n in step_by_var:
            x, y = pos[n]
            ax.annotate(_summary(step_by_var[n]), (x, y - 0.135), fontsize=6.5, ha="center",
                        va="top", color="#9aa5b1")

    # coupling class from the shared classifier (#307): coupled / driven / autonomous self-map.
    summ = function_summary(m)
    label = {"coupled": f"{len(summ['cycles'])} feedback cycle(s) — coupled dynamics",
             "driven": "no feedback cycle — driven pipeline (a driver feeds the cascade)",
             "autonomous": "no cross-coupling — autonomous self-map (closed recurrence)"}[summ["coupling"]]
    sub = f"{len(f['steps'])} functionized vars · {len(drivers)} driver(s) · {label}"
    ax.set_title(f"{m.fsm}  —  compiled data-flow graph\n{sub}", fontsize=12)
    ax.legend(handles=[mpatches.Patch(color=DEP_C, label="dependent (functionized)"),
                       mpatches.Patch(color=DRV_C, label="independent driver"),
                       mpatches.Patch(color=CYC_C, label="feedback edge (coupling)")],
              loc="lower center", ncol=3, fontsize=8, bbox_to_anchor=(0.5, -0.06))
    ax.set_axis_off()
    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)


def _placeholder(out_path, fsm, msg):
    fig, ax = plt.subplots(figsize=(8, 6))
    ax.text(0.5, 0.5, msg, ha="center", va="center", fontsize=13)
    ax.set_axis_off(); ax.set_title(f"{fsm}  —  data-flow graph")
    fig.savefig(out_path, dpi=120, bbox_inches="tight"); plt.close(fig)


if __name__ == "__main__":
    if len(sys.argv) != 4:
        print("usage: render_function_graph.py <smt2> <schema> <out>", file=sys.stderr); sys.exit(2)
    render(sys.argv[1], sys.argv[2], sys.argv[3])
