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
from render_common import (                                # noqa: E402
    GREEN as _GREEN, BLUE as _BLUE, GREY as _GREY, ARROW as _ARROW,
    short as _short, verdict_banner, draw_range_bar, range_extent,
)


def _num(v):
    try:
        return float(v)
    except (TypeError, ValueError):
        return None


# An open-ended bar (half-bounded numeric) extends this far past its finite end before the arrow,
# expressed as a fraction of the drawn value span so the arrow reads at any scale.
_OPEN_FRAC = 0.18


def _rows(r):
    """Unified [(name, kind, lo, hi, numeric, forced_val)] — forced first, then free, top-down.

    `numeric` distinguishes a numeric var unbounded on a side (rng is a 2-tuple with a None end —
    half-bounded, draws an arrow) from a genuinely non-numeric var (rng is None entirely — bool/enum,
    no bar at all). The old code collapsed both via `rng or (None, None)`, which mislabelled a
    half-bounded numeric (a≥3) as 'free (non-numeric)' (#379)."""
    rows = [(n, "forced", None, None, True, v) for n, v in r["backbone"]]
    for n, rng in r["free"]:
        if rng is None:                                   # bool / enum — not a numeric range at all
            rows.append((n, "free", None, None, False, None))
        else:
            rows.append((n, "free", rng[0], rng[1], True, None))
    rows.reverse()
    return rows


def _bar_drawer(ax, y):
    """A `bar(left, width, finite)` callback for render_common.draw_range_bar: this renderer's barh
    style — a bordered blue bar + '[lo, hi]' label when finite (closed), a borderless stub when open
    (the arrow itself is drawn by draw_range_bar)."""
    def bar(left, width, finite):
        if finite:
            ax.barh(y, width or 0.25, left=left, height=0.45, color="#90caf9",
                    edgecolor=_BLUE, lw=1.5, zorder=2)
            ax.annotate(f"[{left:g}, {left + width:g}]", (left + width, y), xytext=(6, 0),
                        textcoords="offset points", va="center", fontsize=9, color=_BLUE)
        else:
            ax.barh(y, width, left=left, height=0.45, color="#90caf9", edgecolor="none", zorder=2)
    return bar


def _draw(ax, r):
    rows = _rows(r)
    stub = range_extent([v for (_, _, lo, hi, _, fv) in rows
                         for v in (lo, hi, _num(fv))]) * _OPEN_FRAC
    labels, colors, reach = [], [], []
    for y, (n, kind, lo, hi, numeric, fv) in enumerate(rows):
        labels.append(_short(n))
        if kind == "forced":
            colors.append(_GREEN)
            fvn = _num(fv)
            x = fvn if fvn is not None else 0
            ax.scatter([x], [y], marker="D", s=130, color=_GREEN, edgecolor="black", zorder=3)
            ax.annotate(f"= {fv}", (x, y), xytext=(9, 0), textcoords="offset points",
                        va="center", color=_GREEN, fontweight="bold", fontsize=10)
        elif not numeric:                                 # genuinely non-numeric (bool / enum)
            colors.append(_BLUE)
            ax.annotate("free (non-numeric)", (0, y), va="center", fontsize=9, color=_BLUE)
        else:                                             # numeric — closed bar OR open-ended arrow (#379)
            colors.append(_BLUE if (lo is not None and hi is not None) else _ARROW)
            reach.append(draw_range_bar(ax, y, lo, hi, stub, _bar_drawer(ax, y)))
    if any(lo is None or hi is None for _, _, lo, hi, num, _ in rows if num):
        x0, x1 = ax.get_xlim()                            # leave headroom so an open end reads as open
        ax.set_xlim(min([x0] + [rr[0] for rr in reach]) - stub * 0.5,
                    max([x1] + [rr[1] for rr in reach]) + stub * 0.5)
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
    elif r.get("note"):
        # #460: the structure couldn't be DETERMINED (nonlinear arithmetic / solver timeout) — surface the
        # honest WHY. Without this the empty backbone/free renders as "0 forced · 0 free · 0 relations",
        # which misreads as "the claim pins nothing down" rather than "the analysis was skipped".
        ax.text(0.5, 0.5, r["note"], ha="center", va="center", fontsize=13, color=_GREY,
                transform=ax.transAxes, wrap=True)
        ax.set_xticks([]); ax.set_yticks([])
        for sp in ax.spines.values():
            sp.set_visible(False)
        msg, col = r["note"], _GREY
    else:
        _draw(ax, r)
        nb, nf = len(r["backbone"]), len(r["free"])
        rels = []
        if r["equalities"]:
            rels.append("forced equal: " + ", ".join(f"{_short(a)}={_short(b)}" for a, b in r["equalities"]))
        if r.get("inequalities"):
            rels.append("forced different: " + ", ".join(f"{_short(a)}≠{_short(b)}" for a, b in r["inequalities"]))
        if r.get("relations"):
            rels.append("implied: " + "; ".join(x["eq"] for x in r["relations"]))  # relations are {eq, core} (#341)
        nrel = len(r["equalities"]) + len(r.get("inequalities", [])) + len(r.get("relations", []))
        reltxt = (" · " + " · ".join(rels)) if rels else ""
        # #379: a numeric free var with a None bound-end is half- (or fully-) unbounded. The card must
        # NOT read as a closed determination — acknowledge the open side(s) so it never implies a finite
        # boundary it doesn't have.
        n_open = sum(1 for _, rng in r["free"]
                     if rng is not None and (rng[0] is None or rng[1] is None))
        opentxt = (f" · {n_open} var{'' if n_open == 1 else 's'} open-ended (unbounded on a side — see ⟶)"
                   if n_open else "")
        # "N relations" (not "implied") — the count spans forced-equal + forced-different + implied-affine,
        # each labelled in the prose; "implied" is only the affine subset, so it'd overclaim the count (#340).
        msg = (f"{nb} forced (backbone) · {nf} free · {nrel} relation"
               f"{'' if nrel == 1 else 's'}{reltxt}{opentxt} — what the claim DETERMINES, solved abstractly (Z3)")
        col = _ARROW if n_open else _GREEN
    verdict_banner(fig, ax, out_path, f"{name} — solution structure", msg, col,
                   rect_bottom=0.06)
