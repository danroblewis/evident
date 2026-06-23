#!/usr/bin/env python3
"""render_function_guards.py — the GUARD DECISION TREES of the piecewise functions.

Diagram 3 of the functionizer family. A Guarded step is a piecewise function — a list of
`guard ⇒ body` branches. Listed flat (diagram 2) they're a case table; but the guards share
conditions, so trie-ing their conjunction atoms recovers the real NESTED decision the solver found:
`is_first_tick?` → 0, else `_timer < 2?` → +1, else `_light == ?` → {Red,Green,Yellow}. That tree is
the branching LOGIC each variable's next value is computed by — the control-flow the JIT compiles.
"""
import sys

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

sys.path.insert(0, "viz")
from evident_viz import load
from functionize import extract_functions


def _build_trie(branches):
    """Trie the guard-atom sequences; bodies hang off the path ends as '_body'."""
    root = {}
    for b in branches:
        node = root
        for atom in b.get("guard_atoms") or [b["guard"]]:
            node = node.setdefault(atom, {})
        node["_body"] = b["body"]
    return root


def _trie_lines(node, prefix=""):
    """Indented tree lines (box-drawing) for a trie node."""
    lines = []
    kids = [(k, v) for k, v in node.items() if k != "_body"]
    for i, (atom, sub) in enumerate(kids):
        last = (i == len(kids) - 1)
        conn = "└─ " if last else "├─ "
        sub_kids = [k for k in sub if k != "_body"]
        if "_body" in sub and not sub_kids:                    # pure leaf: atom → body
            lines.append(f"{prefix}{conn}{atom}   →   {sub['_body']}")
        else:
            lines.append(f"{prefix}{conn}{atom}")
            lines.extend(_trie_lines(sub, prefix + ("    " if last else "│   ")))
    return lines


def render(smt2, schema, out_path):
    m = load(smt2, schema)
    f = extract_functions(m)
    guarded = [s for s in f["steps"] if s["kind"] == "guarded"]
    if not guarded:
        _placeholder(out_path, m.fsm, "no piecewise (guarded) functions — nothing to branch")
        return

    n = len(guarded)
    fig, axes = plt.subplots(1, n, figsize=(6.2 * n, 6.5), squeeze=False)
    fig.patch.set_facecolor("#0f1419")
    for ax, s in zip(axes[0], guarded):
        ax.set_axis_off()
        ax.set_xlim(0, 1); ax.set_ylim(0, 1)
        trie = _build_trie(s["branches"])
        lines = [f"{s['var']}  ="] + _trie_lines(trie)
        ax.text(0.02, 0.97, "\n".join(lines), ha="left", va="top", fontsize=9.5,
                family="monospace", color="#e6edf3")
        ax.set_title(f"{s['var']}   ({len(s['branches'])} branches)", color="#58a6ff", fontsize=12)
    fig.suptitle(f"{m.fsm}  —  guard decision trees (the branching the JIT compiles)",
                 color="#c9d1d9", fontsize=13)
    fig.savefig(out_path, dpi=120, bbox_inches="tight", facecolor="#0f1419")
    plt.close(fig)


def _placeholder(out_path, fsm, msg):
    fig, ax = plt.subplots(figsize=(8, 6)); fig.patch.set_facecolor("#0f1419")
    ax.text(0.5, 0.5, msg, ha="center", va="center", fontsize=13, color="#c9d1d9")
    ax.set_axis_off(); ax.set_title(f"{fsm}  —  guard decision trees", color="#c9d1d9")
    fig.savefig(out_path, dpi=120, bbox_inches="tight", facecolor="#0f1419"); plt.close(fig)


if __name__ == "__main__":
    if len(sys.argv) != 4:
        print("usage: render_function_guards.py <smt2> <schema> <out>", file=sys.stderr); sys.exit(2)
    render(sys.argv[1], sys.argv[2], sys.argv[3])
