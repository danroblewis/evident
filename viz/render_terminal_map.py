"""render_terminal_map.py — the ABSTRACT end-state map: where an FSM can come to rest.

The dynamics views (state_graph, timing_diagram, …) show behavior ACROSS ticks. This is
their end-state counterpart: it shows the TERMINAL SET — the states the FSM can settle
into — computed ABSTRACTLY from the one-step relation via Z3 (terminal_states.py), not by
enumerating the reachable graph. So it answers, for any model:

  * DAEMON      — empty terminal set; the FSM runs indefinitely (care about its ongoing
                  behavior, not an end state). Works even for an unbounded random walk,
                  where the enumerative views can't run at all.
  * TERMINATES  — the FSM can come to rest; the map plots exactly where.

Entry: render(model, out_path).
"""
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt          # noqa: E402

from terminal_states import classify     # noqa: E402

_GREEN = "#2e7d32"
_AMBER = "#b8860b"
_GREY = "#777777"
_RED = "#c62828"


def _name(model):
    return getattr(model, "fsm", None) or "model"


def _short(k):
    return k.split(".")[-1]


def _axis_limits(model, var, vals):
    """[lo, hi] for an axis: the var's PROVEN range if finite, else the terminal points'
    extent — padded to a readable span so a lone end-state isn't zoomed to sub-unit scale."""
    lo = hi = None
    try:
        r = model.proven_range(var)            # proven_range takes the var DICT, not the name
        if r:
            lo, hi = r
    except Exception:
        pass
    if lo is None or hi is None or hi <= lo:
        lo, hi = min(vals), max(vals)
    if hi - lo < 4:
        mid = (lo + hi) / 2.0
        lo, hi = mid - 2, mid + 2
    return lo - 0.6, hi + 0.6


def render(model, out_path):
    c = classify(model)
    verdict, states, note = c["verdict"], c["states"], c.get("note")
    carried = model.carried
    numeric = [v for v in carried if v["kind"] in ("int", "real")]

    fig, ax = plt.subplots(figsize=(8.2, 5.2))

    if verdict == "terminates" and numeric and states:
        if len(numeric) >= 2:
            vx, vy = numeric[0], numeric[1]
            xs = [s[vx["name"]] for s in states]
            ys = [s[vy["name"]] for s in states]
            ax.scatter(xs, ys, marker="s", s=240, color=_RED, edgecolor="black",
                       linewidth=1.2, zorder=3, label="terminal state")
            ax.set_xlabel(_short(vx["name"])); ax.set_ylabel(_short(vy["name"]))
            ax.set_xlim(*_axis_limits(model, vx, xs))
            ax.set_ylim(*_axis_limits(model, vy, ys))
            ax.grid(True, alpha=0.25)
        else:
            vx = numeric[0]
            xs = sorted(s[vx["name"]] for s in states)
            ax.scatter(xs, [0] * len(xs), marker="s", s=300, color=_RED,
                       edgecolor="black", linewidth=1.2, zorder=3)
            for x in xs:
                ax.annotate(str(x), (x, 0), textcoords="offset points", xytext=(0, 16),
                            ha="center", fontsize=11, fontweight="bold")
            lo, hi = _axis_limits(model, vx, xs)
            ax.plot([lo, hi], [0, 0], color=_GREY, lw=1, zorder=1)
            ax.set_xlim(lo, hi)
            ax.set_yticks([]); ax.set_ylim(-1, 1)
            ax.set_xlabel(_short(vx["name"]))
            ax.spines["left"].set_visible(False)
        ax.spines["top"].set_visible(False); ax.spines["right"].set_visible(False)
    elif verdict == "terminates" and states:
        labels = ["  ".join(f"{_short(k)}={v}" for k, v in s.items()) for s in states]
        ax.barh(range(len(labels)), [1] * len(labels), color=_RED, alpha=0.55, zorder=2)
        ax.set_yticks(range(len(labels))); ax.set_yticklabels(labels, fontsize=11)
        ax.set_xticks([]); ax.invert_yaxis()
        for sp in ("top", "right", "bottom"):
            ax.spines[sp].set_visible(False)
    else:
        # daemon (∅) or unknown — an empty end-state map, with an honest centerpiece
        glyph = "∅" if verdict == "daemon" else "?"
        sub = "no terminal states" if verdict == "daemon" else (note or "Z3 could not decide")
        ax.text(0.5, 0.55, glyph, ha="center", va="center", fontsize=72,
                color=_AMBER if verdict == "daemon" else _GREY, transform=ax.transAxes)
        ax.text(0.5, 0.32, sub, ha="center", va="center", fontsize=13,
                color=_AMBER if verdict == "daemon" else _GREY, transform=ax.transAxes)
        ax.set_xticks([]); ax.set_yticks([])
        for sp in ax.spines.values():
            sp.set_visible(False)

    banners = {
        "terminates": (f"TERMINATES — {len(states)} terminal state(s) · the FSM can come "
                       "to rest here · solved abstractly from the one-step relation (Z3), "
                       "not by enumerating runs", _GREEN),
        "daemon": ("DAEMON — no terminal state · the FSM runs indefinitely; its value is "
                   "in the ongoing/recurrent behavior, not an end state · decided abstractly "
                   "(works even when the state space is unbounded)", _AMBER),
        "unknown": ("UNDECIDED — the quantified terminal-set query returned unknown (a hard "
                    "nonlinear/unbounded relation); try a tighter or bounded model", _GREY),
    }
    msg, col = banners[verdict]
    if verdict == "unknown" and note:
        msg = note
    ax.set_title(f"{_name(model)} — terminal-state map  ·  {verdict.upper()}",
                 fontsize=13, fontweight="bold")
    fig.text(0.5, 0.02, msg, ha="center", va="bottom", fontsize=8.5, color=col, wrap=True)
    fig.tight_layout(rect=[0, 0.07, 1, 1])
    fig.savefig(out_path, dpi=120)
    plt.close(fig)
