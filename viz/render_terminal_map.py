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

from terminal_states import classify, stability     # noqa: E402
from render_common import (                          # noqa: E402
    GREEN as _GREEN, AMBER as _AMBER, GREY as _GREY, RED as _RED, ORANGE as _ORANGE,
    short as _short, model_name as _name, empty_panel, verdict_banner,
)

# fixed-point stability → (colour, marker, legend label) for the terminal markers (#20)
_STAB = {
    "stable":   (_GREEN,  "o", "stable (attractor)"),
    "unstable": (_RED,    "X", "unstable (repeller)"),
    "saddle":   (_ORANGE, "D", "saddle"),
    "unknown":  (_GREY,   "s", "terminal"),
}


def _draw_terminals(ax, model, states, numeric):
    """Plot the terminal states coloured by LOCAL STABILITY (#20: a bistable's stable walls 0,6 vs
    its unstable saddle 3). Returns the per-state stability list for the banner."""
    stabs = [stability(model, s, numeric) for s in states]
    seen = set()

    def _style(st):
        col, mk, lbl = _STAB[st]
        lab = lbl if st not in seen else None
        seen.add(st)
        return col, mk, lab

    if len(numeric) >= 2:
        vx, vy = numeric[0], numeric[1]
        for s, st in zip(states, stabs):
            col, mk, lab = _style(st)
            ax.scatter([s[vx["name"]]], [s[vy["name"]]], marker=mk, s=240, color=col,
                       edgecolor="black", linewidth=1.2, zorder=3, label=lab)
        xs = [s[vx["name"]] for s in states]; ys = [s[vy["name"]] for s in states]
        ax.set_xlabel(_short(vx["name"])); ax.set_ylabel(_short(vy["name"]))
        ax.set_xlim(*_axis_limits(model, vx, xs)); ax.set_ylim(*_axis_limits(model, vy, ys))
        ax.grid(True, alpha=0.25)
    else:
        vx = numeric[0]
        xs = [s[vx["name"]] for s in states]
        for s, st in sorted(zip(states, stabs), key=lambda p: p[0][vx["name"]]):
            col, mk, lab = _style(st); x = s[vx["name"]]
            ax.scatter([x], [0], marker=mk, s=300, color=col, edgecolor="black",
                       linewidth=1.2, zorder=3, label=lab)
            ax.annotate(f"{x}\n{st}", (x, 0), textcoords="offset points", xytext=(0, 14),
                        ha="center", fontsize=9, fontweight="bold", color=col)
        lo, hi = _axis_limits(model, vx, xs)
        ax.plot([lo, hi], [0, 0], color=_GREY, lw=1, zorder=1)
        ax.set_xlim(lo, hi); ax.set_yticks([]); ax.set_ylim(-1.2, 1)
        ax.set_xlabel(_short(vx["name"])); ax.spines["left"].set_visible(False)
    ax.legend(loc="upper right", fontsize=8)
    ax.spines["top"].set_visible(False); ax.spines["right"].set_visible(False)
    return stabs


def _cell(s):
    return "(" + ", ".join(f"{_short(k)}={v}" for k, v in s.items()) + ")"


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
    must = c.get("must_rest")
    carried = model.carried
    numeric = [v for v in carried if v["kind"] in ("int", "real")]
    stabs = []

    fig, ax = plt.subplots(figsize=(8.2, 5.2))

    if verdict == "terminates" and numeric and states:
        stabs = _draw_terminals(ax, model, states, numeric)
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
        empty_panel(ax, glyph, sub, _AMBER if verdict == "daemon" else _GREY)

    if must is True:
        term = (f"TERMINATES — EVERY run reaches rest · {len(states)} terminal state(s) · the "
                "non-rest states form a DAG into the absorbing set (#328)", _GREEN)
    elif must is False:
        cyc = c.get("rest_cycle")
        loop = (" · e.g. loops " + "→".join(_cell(s) for s in cyc)) if cyc else ""
        term = (f"CAN REST (not always) — {len(states)} terminal state(s), but a run can loop forever "
                f"among non-rest states{loop}; a cycle avoids the absorbing set (#328, #333)", _AMBER)
    else:
        term = (f"TERMINATES — {len(states)} terminal state(s) · the FSM can come to rest here · "
                "solved abstractly from the one-step relation (Z3), not by enumerating runs", _GREEN)
    banners = {
        "terminates": term,
        "daemon": ("DAEMON — no terminal state · the FSM runs indefinitely; its value is "
                   "in the ongoing/recurrent behavior, not an end state · decided abstractly "
                   "(works even when the state space is unbounded)", _AMBER),
        "unknown": ("UNDECIDED — the quantified terminal-set query returned unknown (a hard "
                    "nonlinear/unbounded relation); try a tighter or bounded model", _GREY),
    }
    msg, col = banners[verdict]
    if verdict == "unknown" and note:
        msg = note
    if stabs:
        parts = [f"{stabs.count(k)} {k}" for k in ("stable", "unstable", "saddle", "unknown")
                 if stabs.count(k)]
        msg += " · " + ", ".join(parts)
    verdict_banner(fig, ax, out_path,
                   f"{_name(model)} — terminal-state map  ·  {verdict.upper()}", msg, col)
