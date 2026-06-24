"""render_solution_structure.py — a claim's ABSTRACT solution-space STRUCTURE.

The "what does this claim actually pin down?" view. From claim_structure.solution_structure (pure Z3,
no sampling) it plots each scalar variable as either a FORCED point (the backbone — green diamond at
its one possible value) or a FREE interval (blue bar over its proven range), with implied equalities
called out in the banner. Goes beyond claim_space's bare ranges by separating determined from free.

Claim-renderer entry: render(smt2_path, schema_path, out_path).
"""
import json

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt                          # noqa: E402

from claim_structure import solution_structure           # noqa: E402

_GREEN = "#2e7d32"
_BLUE = "#1565c0"
_GREY = "#777777"


def _short(n):
    return n.split(".")[-1]


def _num(v):
    try:
        return float(v)
    except (TypeError, ValueError):
        return None


def _rows(r):
    """Unified [(name, kind, lo, hi, forced_val)] — forced first, then free, top-down."""
    rows = [(n, "forced", None, None, v) for n, v in r["backbone"]]
    rows += [(n, "free", (rng or (None, None))[0], (rng or (None, None))[1], None)
             for n, rng in r["free"]]
    rows.reverse()
    return rows


def _draw(ax, r):
    rows = _rows(r)
    labels, colors = [], []
    for y, (n, kind, lo, hi, fv) in enumerate(rows):
        labels.append(_short(n))
        if kind == "forced":
            colors.append(_GREEN)
            fvn = _num(fv)
            x = fvn if fvn is not None else 0
            ax.scatter([x], [y], marker="D", s=130, color=_GREEN, edgecolor="black", zorder=3)
            ax.annotate(f"= {fv}", (x, y), xytext=(9, 0), textcoords="offset points",
                        va="center", color=_GREEN, fontweight="bold", fontsize=10)
        else:
            colors.append(_BLUE)
            if lo is not None and hi is not None:
                ax.barh(y, (hi - lo) or 0.25, left=lo, height=0.45, color="#90caf9",
                        edgecolor=_BLUE, lw=1.5, zorder=2)
                ax.annotate(f"[{lo}, {hi}]", (hi, y), xytext=(6, 0), textcoords="offset points",
                            va="center", fontsize=9, color=_BLUE)
            else:
                ax.annotate("free (non-numeric)", (0, y), va="center", fontsize=9, color=_BLUE)
    ax.set_yticks(range(len(rows))); ax.set_yticklabels(labels, fontsize=11)
    for tick, c in zip(ax.get_yticklabels(), colors):
        tick.set_color(c)
    ax.set_ylim(-0.7, len(rows) - 0.3)
    ax.set_xlabel("value")
    ax.grid(True, axis="x", alpha=0.25)
    for sp in ("top", "right"):
        ax.spines[sp].set_visible(False)


def render(smt2_path, schema_path, out_path):
    name = json.load(open(schema_path)).get("claim", "claim")
    r = solution_structure(smt2_path, schema_path)
    fig, ax = plt.subplots(figsize=(9, 5.5))
    if not r["sat"]:
        ax.text(0.5, 0.5, "UNSATISFIABLE\nno assignment satisfies this claim", ha="center",
                va="center", fontsize=15, color=_GREY, transform=ax.transAxes)
        ax.set_xticks([]); ax.set_yticks([])
        for sp in ax.spines.values():
            sp.set_visible(False)
        msg, col = "no solution structure — the claim is unsatisfiable", _GREY
    else:
        _draw(ax, r)
        nb, nf, ne = len(r["backbone"]), len(r["free"]), len(r["equalities"])
        eqtxt = ""
        if r["equalities"]:
            eqtxt = " · forced equal: " + ", ".join(f"{_short(a)}={_short(b)}" for a, b in r["equalities"])
        msg = (f"{nb} forced (backbone) · {nf} free · {ne} implied equalit"
               f"{'y' if ne == 1 else 'ies'}{eqtxt} — what the claim DETERMINES, solved abstractly (Z3)")
        col = _GREEN
    ax.set_title(f"{name} — solution structure", fontsize=13, fontweight="bold")
    fig.text(0.5, 0.02, msg, ha="center", va="bottom", fontsize=8.5, color=col, wrap=True)
    fig.tight_layout(rect=[0, 0.06, 1, 1])
    fig.savefig(out_path, dpi=120)
    plt.close(fig)
