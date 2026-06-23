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
from functionize import guard_analysis
from render_function_common import cli_main, load_functions, placeholder


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
    m, f = load_functions(smt2, schema)
    guarded = [s for s in f["steps"] if s["kind"] == "guarded"]
    if not guarded:
        placeholder(out_path, m.fsm, "guard decision trees",
                    "no piecewise (guarded) functions — nothing to branch", dark=True)
        return

    try:
        ga = guard_analysis(m, f["steps"], f["residual"])
    except Exception:
        ga = {}
    n = len(guarded)
    fig, axes = plt.subplots(1, n, figsize=(6.2 * n, 6.5), squeeze=False)
    fig.patch.set_facecolor("#0f1419")
    for ax, s in zip(axes[0], guarded):
        ax.set_axis_off()
        ax.set_xlim(0, 1); ax.set_ylim(0, 1)
        trie = _build_trie(s["branches"])
        lines = [f"{s['var']}  ="] + _trie_lines(trie)
        ax.text(0.02, 0.93, "\n".join(lines), ha="left", va="top", fontsize=9.5,
                family="monospace", color="#e6edf3")
        # z3 verdict: is this piecewise dispatch a TOTAL, UNAMBIGUOUS function?
        v = ga.get(s["var"])
        if v is not None:
            if v["complete"] and v["disjoint"]:
                badge, col = "✓ total & unambiguous (over the declared type domain)", "#3fb950"
            elif not v["complete"]:
                w = v.get("gap_witness")
                badge = f"⚠ INCOMPLETE — no branch for  {w}" if w else "⚠ INCOMPLETE — some input hits no branch"
                col = "#d29922"
            else:
                w = v.get("overlap_witness")
                badge = (f"⚠ OVERLAPPING {v['overlap']} — both fire at  {w}" if w
                         else f"⚠ OVERLAPPING guards {v['overlap']} — ambiguous dispatch")
                col = "#d29922"
            ax.text(0.02, 0.985, badge, ha="left", va="top", fontsize=8, color=col, weight="bold")
        ax.set_title(f"{s['var']}   ({len(s['branches'])} branches)", color="#58a6ff", fontsize=12)
    fig.suptitle(f"{m.fsm}  —  guard decision trees (the branching the JIT compiles)\n"
                 "verdict checked over the DECLARED type domain (∩ type invariants), not the reachable "
                 "set — a ⚠ may name an input the system never actually reaches",
                 color="#c9d1d9", fontsize=11)
    fig.savefig(out_path, dpi=120, bbox_inches="tight", facecolor="#0f1419")
    plt.close(fig)


if __name__ == "__main__":
    cli_main(render, sys.argv, "render_function_guards.py")
