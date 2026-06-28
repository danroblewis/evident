"""render_common.py — shared primitives for the abstract-analysis renderers.

The dynamics/claim end-state views (render_terminal_map, render_reachable_region,
render_solution_structure, …) all share the same scaffolding: a fixed palette, the
"last segment of a dotted name" shortener, the model display name, an empty-panel
centerpiece for the ∅/?/unsat case, and the verdict-banner tail (title + bottom
caption + tight_layout + savefig). This module owns those so each renderer carries
only its own drawing logic.

Behavior note: these are byte-for-byte the shapes the renderers used inline — same
hex colours, same font sizes, same banner geometry — so lifting them changes no PNG.
"""

import contextlib

import matplotlib.figure
from matplotlib.patches import Rectangle as _Rect

# Shared palette — the verdict/stability colours used across the abstract renderers.
# #469: the verdict/stability palette, brightened to the page's own colours (app.css --good /
# --warn / --bad / --accent) so text + region edges read on the DARK IDE page instead of the
# muted-on-dark saturated tones tuned for a white background.
GREEN = "#3fb950"     # --good
AMBER = "#d29922"     # --warn
GREY = "#9aa3ad"      # a light grey (the old #777 vanished on dark)
RED = "#f85149"       # --bad
ORANGE = "#ff7b39"    # a brighter orange (legible on dark)
BLUE = "#58a6ff"      # --accent


ARROW = "#ff7b39"     # ORANGE — the "this side is unbounded" colour, legible on the DARK page (#379)


def range_extent(endpoints):
    """A representative value span across a flat list of (possibly-None) endpoints — sizes the
    open-ended arrow stubs so they read at the panel's scale even when only one finite bound exists."""
    vals = [v for v in endpoints if v is not None]
    if not vals:
        return 1.0
    return (max(vals) - min(vals)) or max(1.0, abs(vals[0]))


def draw_range_bar(ax, y, lo, hi, stub, bar):
    """Draw one numeric variable's solved range on a horizontal lane (#379). A closed range is a finite
    `bar(left, width)`; a half- (or fully-) unbounded range is a short stub with an ORANGE arrow running
    OFF the open side(s) — '≥ lo' / '≤ hi' / '(-∞, +∞)' — so an unbounded side reads as an arrowhead,
    never a closed boundary and never a fabricated witness endpoint. `bar(left, width, finite)` draws the
    renderer's own bar (closed style when finite=True, open stub when False) and labels the finite end.
    Returns (xmin, xmax) the drawn elements reach, so the caller can leave headroom on the open side."""
    if lo is not None and hi is not None:                 # closed exact range
        bar(lo, (hi - lo), True)
        return (lo, hi)
    if lo is not None:                                    # ≥ lo — bounded below, open above
        bar(lo, stub, False)
        ax.annotate("", (lo + stub * 2.4, y), (lo, y),
                    arrowprops=dict(arrowstyle="-|>", color=ARROW, lw=2.4))
        ax.annotate(f"≥ {lo:g}  [{lo:g}, +∞)", (lo + stub * 2.4, y), xytext=(8, 0),
                    textcoords="offset points", va="center", fontsize=9, color=ARROW)
        return (lo, lo + stub * 4.2)
    if hi is not None:                                    # ≤ hi — bounded above, open below
        bar(hi - stub, stub, False)
        ax.annotate("", (hi - stub * 2.4, y), (hi, y),
                    arrowprops=dict(arrowstyle="-|>", color=ARROW, lw=2.4))
        ax.annotate(f"≤ {hi:g}  (-∞, {hi:g}]", (hi, y), xytext=(8, 0),
                    textcoords="offset points", va="center", fontsize=9, color=ARROW)
        return (hi - stub * 2.4, hi + stub * 2.6)
    base = stub if stub else 1.0                          # unbounded both sides — no finite endpoint
    ax.annotate("", (base * 2.4, y), (-base * 2.4, y),
                arrowprops=dict(arrowstyle="<|-|>", color=ARROW, lw=2.4))
    ax.annotate("(-∞, +∞)  unbounded", (base * 2.4, y), xytext=(8, 0),
                textcoords="offset points", va="center", fontsize=9, color=ARROW)
    return (-base * 2.6, base * 4.6)


def short(name):
    """Last segment of a dotted variable name (`state.pos.x` → `x`)."""
    return name.split(".")[-1]


def model_name(model):
    """Display name for a model: its FSM name, or a generic fallback."""
    return getattr(model, "fsm", None) or "model"


def empty_panel(ax, glyph, sub, color):
    """The honest centerpiece for a verdict with nothing to plot (∅ daemon, ? undecided):
    a large glyph over a one-line reason, with ticks and all spines stripped."""
    ax.text(0.5, 0.55, glyph, ha="center", va="center", fontsize=72,
            color=color, transform=ax.transAxes)
    ax.text(0.5, 0.32, sub, ha="center", va="center", fontsize=13,
            color=color, transform=ax.transAxes)
    ax.set_xticks([])
    ax.set_yticks([])
    for sp in ax.spines.values():
        sp.set_visible(False)


# --- BROKEN-model de-rating (#416) ----------------------------------------------------
# A model with dropped constraints (the user wrote a constraint that did NOT translate to a
# Z3 Bool — a typo'd identifier, a precedence slip) is UNDER-CONSTRAINED: its diagram is a
# picture of a DIFFERENT, looser relation than the one the source describes. The honesty used
# to ride only in the TEXT around the plot (banner/badge/footer); the PLOT itself drew clean,
# authoritative styling. This makes the PICTURE itself say "provisional": a faint diagonal-hatch
# scrim over the data region + one anchored chip, so an image-scanner (the tool trains the user
# to read the diagram as the debugger) registers "don't trust this" without the text.
#
# It is applied UNIFORMLY across every renderer by patching Figure.savefig (below) to call this
# on the figure's data axes when `_BROKEN` is set — so no per-renderer change is needed and a new
# renderer is de-rated for free. The scrim is faint (data stays readable); the chip is the loud part.

_BROKEN = 0          # dropped-constraint count for the in-flight render; 0 = healthy, no de-rating


def derate_broken(fig, dropped):
    """Overlay a faint diagonal-hatch scrim over each data axes of `fig` plus one
    'PROVISIONAL — N constraint(s) dropped' chip, marking the whole picture as the diagram of an
    UNDER-CONSTRAINED model (#416). Faint enough that the data stays legible; the chip is the loud
    signal. A no-op when `dropped` is 0. Drawn on the dark IDE theme — amber hatch + amber chip read
    over #0f1419. Skips axes with nothing drawn (an empty/off panel) so a legend-only axes isn't hatched."""
    if not dropped:
        return
    data_axes = [ax for ax in fig.axes if ax.get_visible() and ax.axison
                 and (ax.has_data() or ax.patches or ax.texts)]
    if not data_axes:
        data_axes = [ax for ax in fig.axes if ax.get_visible()]
    for ax in data_axes:
        # A faint diagonal hatch spanning the axes in axes-fraction coords: low alpha so the
        # underlying data shows through, but the texture reads "provisional" at a glance.
        ax.add_patch(_Rect((0, 0), 1, 1, transform=ax.transAxes, zorder=50,
                           facecolor="none", edgecolor=AMBER, hatch="////",
                           linewidth=0, alpha=0.22, clip_on=False))
    # One chip, anchored on the largest data axes (the lead panel), top-left inside the data.
    lead = max(data_axes, key=lambda a: a.get_position().width * a.get_position().height)
    lead.text(0.5, 0.97,
              f"PROVISIONAL — {dropped} constraint(s) dropped",
              transform=lead.transAxes, ha="center", va="top", zorder=60,
              fontsize=10, fontweight="bold", color="#0f1419",
              bbox=dict(boxstyle="round,pad=0.35", facecolor=AMBER, edgecolor="none", alpha=0.95))


def _patched_savefig(self, *args, **kwargs):
    """Figure.savefig wrapper that de-rates the picture (#416) just before saving whenever a
    BROKEN model is in flight (`_BROKEN > 0`, set by the render layer's broken-context). Uniform
    across every renderer — they all call `fig.savefig` — so no renderer carries the de-rating logic."""
    if _BROKEN:
        try:
            derate_broken(self, _BROKEN)
        except Exception:
            pass          # de-rating must never sink a render — the surrounding text honesty remains
    return _ORIG_SAVEFIG(self, *args, **kwargs)


def verdict_banner(fig, ax, out_path, title, msg, col, rect_bottom=0.07):
    """The common tail of an abstract-analysis render: bold title, a wrapped caption
    pinned to the bottom in the verdict colour, a tight layout reserving room for it,
    then save at dpi 120 and close. `rect_bottom` is the fraction reserved for the
    caption (renderers with a taller caption pass a smaller value)."""
    ax.set_title(title, fontsize=13, fontweight="bold")
    fig.text(0.5, 0.02, msg, ha="center", va="bottom", fontsize=8.5, color=col, wrap=True)
    fig.tight_layout(rect=[0, rect_bottom, 1, 1])
    fig.savefig(out_path, dpi=120)
    import matplotlib.pyplot as plt
    plt.close(fig)


# Install the savefig de-rating hook ONCE at import (render_common is imported by every renderer
# via evident_viz). Idempotent — guarded so a re-import doesn't double-wrap.
if not getattr(matplotlib.figure.Figure.savefig, "_evident_derated", False):
    _ORIG_SAVEFIG = matplotlib.figure.Figure.savefig
    _patched_savefig._evident_derated = True
    matplotlib.figure.Figure.savefig = _patched_savefig


@contextlib.contextmanager
def broken_render(dropped):
    """Set the in-flight BROKEN dropped-count for the duration of one render (the render layer
    wraps each `_render_png` call with this when the model has dropped constraints), then restore.
    A no-op de-rating when `dropped` is 0 — healthy models render clean (#416). Serialized by the
    server's render _LOCK, so the module flag never overlaps another request."""
    global _BROKEN
    saved = _BROKEN
    _BROKEN = int(dropped or 0)
    try:
        yield
    finally:
        _BROKEN = saved
