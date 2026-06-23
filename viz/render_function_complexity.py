#!/usr/bin/env python3
"""render_function_complexity.py — the JIT's-eye COST of each compiled function.

The 6th functionizer view. Each per-variable function carries a compilation weight: a Scalar is its
arithmetic; a Guarded function is its branches plus the guards and bodies inside them. This ranks the
functions by that weight — which variables are cheap (a constant or a one-op recurrence) vs expensive
(a deep guarded function with big expressions) to compute every tick. The JIT's-eye view of where the
program's per-tick work actually goes — invisible in the dynamics views, which show the result not the cost.
"""
import sys

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

sys.path.insert(0, "viz")
from render_function_common import cli_main, load_functions, placeholder

OPS = ("+", "-", "*", "/")


def _ops(s):
    return sum(s.count(op) for op in OPS)


def _cost(step):
    """(branches, ops, weight) for a function. Branches cost 2 each (a compare + a select); ops are
    the arithmetic the JIT emits; guard atoms are the comparisons it tests."""
    if step["kind"] == "scalar":
        o = _ops(step["expr"])
        return 0, o, o + 1
    nb = len(step["branches"])
    o = sum(_ops(b["body"]) + len(b.get("guard_atoms") or [b["guard"]]) for b in step["branches"])
    return nb, o, nb * 2 + o


def render(smt2, schema, out_path):
    m, f = load_functions(smt2, schema)
    rows = []
    for s in f["steps"]:
        nb, o, w = _cost(s)
        rows.append((s["var"], s["kind"], nb, o, w))
    if not rows:
        placeholder(out_path, m.fsm, "function cost", "no functionized variables to weigh"); return
    rows.sort(key=lambda r: r[4])                       # lightest at the bottom

    fig, ax = plt.subplots(figsize=(9, max(3.0, 0.62 * len(rows) + 1.6)))
    y = range(len(rows))
    branch_w = [r[2] * 2 for r in rows]
    op_w = [r[3] for r in rows]
    ax.barh(list(y), branch_w, color="#8957e5", label="branching (guards × 2)")
    ax.barh(list(y), op_w, left=branch_w, color="#1f6feb", label="arithmetic ops")
    ax.set_yticks(list(y))
    ax.set_yticklabels([f"{r[0]}" for r in rows], fontsize=10)
    for i, r in enumerate(rows):
        kind = "piecewise" if r[1] == "guarded" else "scalar"
        ax.text(r[4] + 0.15, i, f"  {kind}: {r[2]} branch · {r[3]} ops  (cost {r[4]})",
                va="center", fontsize=8, color="#7d8590")
    ax.set_xlabel("AST weight  (branches × 2 + arithmetic ops — a STRUCTURAL proxy, not Cranelift's op count)")
    ax.set_title(f"{m.fsm}  —  function cost (AST-size heuristic — relative weight from the z3 AST, "
                 f"NOT measured from the JIT)", fontsize=11)
    ax.legend(loc="lower right", fontsize=8)
    ax.margins(x=0.18)
    fig.savefig(out_path, dpi=120, bbox_inches="tight")
    plt.close(fig)


if __name__ == "__main__":
    cli_main(render, sys.argv, "render_function_complexity.py")
