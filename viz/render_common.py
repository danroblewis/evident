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

# Shared palette — the verdict/stability colours used across the abstract renderers.
GREEN = "#2e7d32"
AMBER = "#b8860b"
GREY = "#777777"
RED = "#c62828"
ORANGE = "#e65100"
BLUE = "#1565c0"


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
