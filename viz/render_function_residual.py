#!/usr/bin/env python3
"""render_function_residual.py — the FUNCTIONIZED-vs-RESIDUAL boundary.

Diagram 2 of the functionizer family, and the most constraint-language-native picture in the tool.
The functionizer reduces what it can to COMPUTATION (per-variable functions — Scalar/Guarded) and
leaves the rest as genuine CONSTRAINTS (the residual checks/predicates — typically invariants like
`0 ≤ timer ≤ 2`). This draws that split as two columns: ⚙ functions on the left (the update law the
JIT runs), ⊓ residual constraints on the right (the relational part that never became a function).

The headline ratio — "k of n constraints reduced to functions" — is a one-glance answer to "how much
of my relational program is actually computation, and where is it still truly relational?"
"""
import sys
import textwrap

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

sys.path.insert(0, "viz")
from functionize import function_summary
from render_function_common import cli_main, load_functions

FN_C = "#16313f"; FN_E = "#3fb950"      # function box fill / edge (computed — green)
RS_C = "#2a1d12"; RS_E = "#d29922"      # residual box fill / edge (constraint — amber)


def _fn_text(step):
    if step["kind"] == "scalar":
        return f"{step['var']}  =  {step['expr']}"
    lines = [f"{step['var']}   (piecewise — {len(step['branches'])} branches)"]
    for b in step["branches"][:4]:
        lines.append(f"   {b['guard']}  ⇒  {b['body']}")
    if len(step["branches"]) > 4:
        lines.append(f"   … {len(step['branches']) - 4} more")
    return "\n".join(lines)


def _col(ax, x, w, title, items, fill, edge, empty):
    ax.text(x + w / 2, 0.965, title, ha="center", va="top", fontsize=12, color=edge, weight="bold")
    y = 0.91
    if not items:
        ax.text(x + w / 2, 0.5, empty, ha="center", va="center", fontsize=10, color="#7d8590", style="italic")
        return
    for txt in items:
        wrapped = "\n".join("\n".join(textwrap.wrap(ln, 46)) or ln for ln in txt.split("\n"))
        nlines = wrapped.count("\n") + 1
        h = 0.022 * nlines + 0.03
        ax.add_patch(plt.Rectangle((x, y - h), w, h, facecolor=fill, edgecolor=edge,
                                   linewidth=1.4, zorder=1))
        ax.text(x + 0.012, y - 0.018, wrapped, ha="left", va="top", fontsize=8.5,
                color="#e6edf3", family="monospace", zorder=2)
        y -= h + 0.022
        if y < 0.05:
            ax.text(x + w / 2, 0.03, "…", ha="center", fontsize=12, color="#7d8590"); break


def render(smt2, schema, out_path):
    m, f = load_functions(smt2, schema)
    summ = function_summary(m)
    fn_items = [_fn_text(s) for s in f["steps"]]
    rs_items = [r["expr"] for r in f["residual"]]
    # HONEST framing (Ana #305): "% computed" = carried vars WITH an update law / total carried. The
    # residual is STANDING INVARIANTS (type bounds always true), NOT un-computed work — so a 100%-
    # functionized program with type bounds reads "100% · 2 standing invariants", never "50%".
    nfc, nc, n_resid = summ["n_func_carried"], summ["n_carried"], len(f["residual"])
    inv = f" · {n_resid} standing invariant(s)" if n_resid else ""

    fig, ax = plt.subplots(figsize=(11, 7.5))
    ax.set_xlim(0, 1); ax.set_ylim(0, 1); ax.set_axis_off()
    ax.set_title(f"{m.fsm}  —  what the solver compiled\n"
                 f"{nfc} of {nc} carried var(s) have an update law — {summ['pct']:.0f}% computed{inv}",
                 fontsize=12, color="#c9d1d9")
    ax.axvline(0.5, color="#2b3138", linewidth=1)
    _col(ax, 0.02, 0.46, "⚙ FUNCTIONS  (computed)",
         fn_items, FN_C, FN_E, "nothing functionized")
    _col(ax, 0.52, 0.46, "⊓ RESIDUAL  (invariants)",
         rs_items, RS_C, RS_E, "none — the whole transition reduced to functions")
    fig.savefig(out_path, dpi=120, bbox_inches="tight", facecolor="#0f1419")
    plt.close(fig)


if __name__ == "__main__":
    cli_main(render, sys.argv, "render_function_residual.py")
