"""diagram — a generic diagram generator for Evident programs.

It does NOT take a Python model; it takes a real .ev FILE + schema, samples the
schema's INTERFACE variables through the runtime (evident.py sample --json), infers
each variable's kind from the sampled values, and renders the diagram that fits the
interface — letting the types drive the layout (see docs/design/observability.md and
phase-portraits.md). Honours an optional `-- @plot x= y= color= type= title=`
annotation on the schema.

Dispatch (when no explicit @plot type):
  2 numeric                -> scatter (the constraint shape), coloured by an enum
  >2 numeric               -> projection matrix (every pairwise scatter)
  1 numeric + an enum      -> strip plot
  1 numeric                -> histogram (the realistic range)
  only categorical (enums) -> count bars, one panel per variable
"""
import json
import os
import re
import subprocess
import sys
from itertools import combinations
from math import ceil

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
INK = "#2a2c34"; MUTED = "#6b7080"
PALETTE = ["#2868d2", "#2eb55f", "#eb9628", "#8a4fbf", "#de3c3c", "#0e8a8a",
           "#b8398a", "#6b7a8f", "#c0a020", "#3a9ad9"]


# ── runtime access ───────────────────────────────────────────────────────────
def _run(args, timeout=120):
    return subprocess.run([sys.executable] + args, cwd=ROOT, capture_output=True,
                          text=True, timeout=timeout)


def list_schemas(evfile):
    """SAT schemas in a file (parsed from `evident check`)."""
    r = _run(["evident.py", "check", evfile])
    return [m.group(1) for line in r.stdout.splitlines()
            if (m := re.match(r"\s*[✓]\s+([A-Za-z_]\w*)", line))]


def sample(evfile, schema, n=70):
    r = _run(["evident.py", "sample", evfile, schema, "-n", str(n), "--json"])
    try:
        return json.loads(r.stdout)
    except (json.JSONDecodeError, ValueError):
        return []


# ── interface inference ──────────────────────────────────────────────────────
def classify(samples):
    """Each interface var -> (kind, values). kind in real/int/bool/enum/string."""
    keys, out = [], {}
    for s in samples:
        for k in s:
            if k not in keys:
                keys.append(k)
    for k in keys:
        vals = [s[k] for s in samples if k in s]
        if not vals:
            continue
        if all(isinstance(v, bool) for v in vals):
            kind = "bool"
        elif all(isinstance(v, (int, float)) and not isinstance(v, bool) for v in vals):
            kind = "real" if any(isinstance(v, float) and v != int(v) for v in vals) else "int"
        elif all(isinstance(v, str) for v in vals):
            kind = "enum" if len(set(vals)) <= 12 else "string"
        else:
            kind = "other"
        out[k] = (kind, vals)
    return out


def plot_annotation(source, schema):
    lines = source.splitlines()
    for i, l in enumerate(lines):
        if re.match(rf"\s*(schema|claim)\s+{re.escape(schema)}\b", l):
            ann = {}
            for j in range(max(0, i - 3), min(len(lines), i + 5)):
                m = re.search(r"@plot\s+(.*)", lines[j])
                if m:
                    for key, val in re.findall(r'(\w+)=("[^"]*"|\S+)', m.group(1)):
                        ann[key] = val.strip('"')
            return ann
    return {}


# ── renderers ────────────────────────────────────────────────────────────────
def _style(ax, xl="", yl=""):
    ax.set_xlabel(xl, fontsize=9, color=MUTED); ax.set_ylabel(yl, fontsize=9, color=MUTED)
    ax.tick_params(colors=MUTED, labelsize=8)
    for s in ax.spines.values():
        s.set_color("#d2d6de")


def _enum_colors(vals):
    cats = sorted(set(vals))
    cmap = {c: PALETTE[i % len(PALETTE)] for i, c in enumerate(cats)}
    return [cmap[v] for v in vals], cmap


def _scatter(ax, xs, ys, xl, yl, color_vals=None):
    if color_vals is not None:
        cols, cmap = _enum_colors(color_vals)
        ax.scatter(xs, ys, c=cols, s=16, alpha=0.6, edgecolors="none")
        for c, col in cmap.items():
            ax.scatter([], [], c=col, label=str(c), s=16)
        ax.legend(fontsize=7, framealpha=0.9, loc="best")
    else:
        ax.scatter(xs, ys, s=16, alpha=0.55, color=PALETTE[0], edgecolors="none")
    _style(ax, xl, yl)


def _vals(vmap, k):
    return vmap[k][1]


def schema_source(source, schema, maxlines=36):
    """The .ev source block for one schema/claim/type, so the diagram shows the
    code it came from. From the declaration line to the next top-level decl."""
    lines = source.splitlines()
    start = next((i for i, l in enumerate(lines)
                  if re.match(rf"\s*(schema|claim|type)\s+{re.escape(schema)}\b", l)), None)
    if start is None:
        return f"-- {schema}: (definition not in this file — imported)"
    base = len(lines[start]) - len(lines[start].lstrip())
    block = [lines[start]]
    for l in lines[start + 1:]:
        if l.strip() and (len(l) - len(l.lstrip())) <= base \
                and re.match(r"\s*(schema|claim|type|assert|import)\b", l):
            break
        block.append(l)
    while block and not block[-1].strip():
        block.pop()
    if len(block) > maxlines:
        block = block[:maxlines] + ["    -- … (truncated)"]
    return "\n".join(block)


# ── panel builders (each returns a draw(ax) closure) ─────────────────────────
def _scatter_panel(vmap, a, b, color_vals, legend=True):
    def draw(ax):
        _scatter(ax, _vals(vmap, a), _vals(vmap, b), a, b, color_vals)
        if not legend and ax.get_legend():
            ax.get_legend().remove()
    return draw


def _bar_panel(vmap, k):
    def draw(ax):
        order = sorted(set(_vals(vmap, k)))
        counts = [_vals(vmap, k).count(c) for c in order]
        ax.bar(range(len(order)), counts,
               color=[PALETTE[i % len(PALETTE)] for i in range(len(order))])
        ax.set_xticks(range(len(order)))
        ax.set_xticklabels(order, fontsize=7, rotation=30, ha="right")
        ax.set_title(k, fontsize=9, color=INK); _style(ax)
    return draw


def _hist_panel(vmap, k):
    def draw(ax):
        ax.hist(_vals(vmap, k), bins=20, color=PALETTE[0], alpha=0.85)
        _style(ax, k, "count")
    return draw


def _strip_panel(samples, vmap, cat, k):
    def draw(ax):
        order = sorted(set(_vals(vmap, cat)))
        for i, c in enumerate(order):
            ys = [s[k] for s in samples if s.get(cat) == c]
            ax.scatter([i] * len(ys), ys, s=14, alpha=0.6,
                       color=PALETTE[i % len(PALETTE)])
        ax.set_xticks(range(len(order)))
        ax.set_xticklabels(order, fontsize=7, rotation=30, ha="right")
        _style(ax, cat, k)
    return draw


def render(samples, vmap, ann, title, code):
    numeric = [k for k, (knd, _) in vmap.items() if knd in ("real", "int")]
    cats = [k for k, (knd, _) in vmap.items() if knd in ("enum", "bool")]
    color_by = ann.get("color") or (cats[0] if cats else None)
    color_vals = _vals(vmap, color_by) if color_by in vmap else None
    typ = ann.get("type")

    # decide the diagram: a list of panel-draw closures + grid shape + caption
    if ann.get("x") in vmap and ann.get("y") in vmap:
        panels = [_scatter_panel(vmap, ann["x"], ann["y"], color_vals)]
        sub = "scatter (annotated x/y)"
    elif typ == "bars" or (not numeric and cats):
        panels = [_bar_panel(vmap, k) for k in cats]
        sub = f"count bars ({len(cats)} categorical vars)"
    elif len(numeric) == 1 and cats:
        panels = [_strip_panel(samples, vmap, cats[0], numeric[0])]
        sub = f"strip ({cats[0]} × {numeric[0]})"
    elif len(numeric) == 1:
        panels = [_hist_panel(vmap, numeric[0])]
        sub = "histogram (realistic range)"
    elif len(numeric) == 2:
        panels = [_scatter_panel(vmap, numeric[0], numeric[1], color_vals)]
        sub = "scatter"
    elif len(numeric) > 2:
        pairs = list(combinations(numeric, 2))
        panels = [_scatter_panel(vmap, a, b, color_vals, legend=False) for a, b in pairs]
        sub = f"projection matrix ({len(numeric)} vars, {len(pairs)} pairs)"
    else:
        panels = [lambda ax: (ax.axis("off"),
                              ax.text(0.5, 0.5, "no numeric/enum interface",
                                      ha="center", color=MUTED))]
        sub = "—"

    cols = 1 if len(panels) == 1 else min(3, len(panels))
    rows = ceil(len(panels) / cols)
    # figure: diagram grid on the left, the source code card on the right
    nloc = code.count("\n") + 1
    W = cols * 3.7 + 5.2
    H = max(rows * 3.3 + 1.0, nloc * 0.2 + 1.4)
    fig = plt.figure(figsize=(W, H))
    outer = fig.add_gridspec(1, 2, width_ratios=[cols * 3.7, 4.6], wspace=0.06,
                             left=0.05, right=0.975, top=0.88, bottom=0.06)
    grid = outer[0].subgridspec(rows, cols, hspace=0.55, wspace=0.34)
    for i, draw in enumerate(panels):
        draw(fig.add_subplot(grid[i // cols, i % cols]))
    for j in range(len(panels), rows * cols):
        fig.add_subplot(grid[j // cols, j % cols]).axis("off")

    cax = fig.add_subplot(outer[1]); cax.axis("off")
    cax.text(0.0, 1.0, code, transform=cax.transAxes, va="top", ha="left",
             family="monospace", fontsize=8, color=INK, linespacing=1.35,
             bbox=dict(boxstyle="round,pad=0.6", fc="#fbfcfe", ec="#cfd3dd", lw=1.2))

    fig.suptitle(title, fontsize=14, color=INK, weight="bold", x=0.05, ha="left")
    fig.text(0.05, 0.915, f"{sub}   ·   {len(samples)} samples of the interface  "
             "·  source on the right", fontsize=9, color=MUTED)
    return fig


def generate(evfile, schema, source, outdir, n=70):
    samples = sample(evfile, schema, n)
    if not samples:
        return None
    vmap = classify(samples)
    if not vmap:
        return None
    ann = plot_annotation(source, schema)
    base = os.path.splitext(os.path.basename(evfile))[0]
    title = ann.get("title", f"{base} · {schema}")
    fig = render(samples, vmap, ann, title, schema_source(source, schema))
    os.makedirs(outdir, exist_ok=True)
    path = os.path.join(outdir, f"{base}__{schema}.png")
    fig.savefig(path, dpi=120, facecolor="white"); plt.close(fig)
    return path


if __name__ == "__main__":   # quick single-schema test
    ev, sch = sys.argv[1], sys.argv[2]
    src = open(os.path.join(ROOT, ev)).read()
    print(generate(ev, sch, src, os.path.join(ROOT, "viz", "diagrams")))
