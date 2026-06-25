"""viz_theme — the ONE matplotlib theme every Evident diagram renders under (#469).

The web IDE page is DARK (app.css `--bg: #0f1419`, text `--ink: #c9d1d9`). A diagram on the
default WHITE matplotlib background clashes and a black-on-white axis is invisible once the page
shows through. So this module, imported once at load time (by evident_viz, which EVERY renderer
imports), sets rcParams so that:

  * the figure / axes backgrounds are TRANSPARENT (`'none'`) and `savefig.facecolor`/`edgecolor`
    are `'none'` too — so a plain `fig.savefig(out)` writes an RGBA PNG with an alpha-0 background
    and the dark page shows through. No renderer has to pass `transparent=True`.
  * every CHROME element (text, axis spines, ticks, labels, title, grid, legend) is a LIGHT color
    drawn from the page's own palette, so it's legible over the dark page instead of black-on-dark.

Colours are the page's: INK (#c9d1d9, the editor text) for primary text, DIM (#7d8590) for
secondary/grid, LINE (#2b3138) lightened for spines. Renderers that hardcode 'k'/'black'/'#000'
or a white fill still need per-file fixes (those override the theme); this sets the DEFAULTS so a
renderer that just uses the matplotlib defaults is already dark-correct.

Import for its side effect: `import viz_theme`  (or `from viz_theme import PAGE_BG, INK, …`)."""
import matplotlib

# The page palette (app.css :root) — keep in sync if the IDE theme changes.
PAGE_BG = "#0f1419"   # --bg: the dark page background diagrams are composited over
PANEL = "#161b22"     # --panel
LINE = "#2b3138"      # --line
INK = "#c9d1d9"       # --ink: primary text / strong foreground
DIM = "#7d8590"       # --dim: secondary text / grid
ACCENT = "#58a6ff"    # --accent
GOOD = "#3fb950"      # --good
WARN = "#d29922"      # --warn
BAD = "#f85149"       # --bad

# Spines a touch lighter than --line so the plot frame reads on the dark page without shouting.
_SPINE = "#3a424c"


def apply():
    """Install the dark-page theme into matplotlib's global rcParams. Idempotent — safe to call
    more than once (the renderers import the module, which calls this at import)."""
    matplotlib.rcParams.update({
        # --- transparency: a plain savefig writes an alpha-0 background (the dark page shows through)
        "figure.facecolor": "none",
        "axes.facecolor": "none",
        "savefig.facecolor": "none",
        "savefig.edgecolor": "none",
        "savefig.transparent": True,
        # --- chrome in the page's LIGHT colours so nothing is black-on-dark
        "text.color": INK,
        "axes.edgecolor": _SPINE,
        "axes.labelcolor": INK,
        "axes.titlecolor": INK,
        "xtick.color": DIM,
        "ytick.color": DIM,
        "xtick.labelcolor": INK,
        "ytick.labelcolor": INK,
        "grid.color": LINE,
        "legend.edgecolor": _SPINE,
        "legend.labelcolor": INK,
        # a faint panel tint behind a legend so light text in it stays readable over busy plots
        "legend.facecolor": PANEL,
        "legend.framealpha": 0.85,
    })


apply()
