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


def render(samples, vmap, ann, title):
    numeric = [k for k, (knd, _) in vmap.items() if knd in ("real", "int")]
    cats = [k for k, (knd, _) in vmap.items() if knd in ("enum", "bool")]
    color_by = ann.get("color") or (cats[0] if cats else None)
    color_vals = _vals(vmap, color_by) if color_by in vmap else None
    typ = ann.get("type")

    # explicit annotation x/y wins
    if ann.get("x") in vmap and ann.get("y") in vmap:
        fig, ax = plt.subplots(figsize=(6.4, 5.6))
        _scatter(ax, _vals(vmap, ann["x"]), _vals(vmap, ann["y"]),
                 ann["x"], ann["y"], color_vals)
        sub = "scatter"
    elif typ == "bars" or (not numeric and cats):
        cols = min(3, len(cats)); rows = (len(cats) + cols - 1) // cols
        fig, axes = plt.subplots(rows, cols, figsize=(cols * 4, rows * 3.2),
                                 squeeze=False)
        for ax, k in zip(axes.ravel(), cats):
            order = sorted(set(_vals(vmap, k)))
            counts = [_vals(vmap, k).count(c) for c in order]
            ax.bar(range(len(order)), counts,
                   color=[PALETTE[i % len(PALETTE)] for i in range(len(order))])
            ax.set_xticks(range(len(order))); ax.set_xticklabels(order, fontsize=7,
                                                                 rotation=30, ha="right")
            ax.set_title(k, fontsize=9, color=INK); _style(ax)
        for ax in axes.ravel()[len(cats):]:
            ax.axis("off")
        sub = f"count bars ({len(cats)} categorical vars)"
    elif len(numeric) == 1:
        k = numeric[0]
        fig, ax = plt.subplots(figsize=(6.4, 4.6))
        if cats:
            for i, c in enumerate(sorted(set(_vals(vmap, cats[0])))):
                ys = [s[k] for s in samples if s.get(cats[0]) == c]
                ax.scatter([i] * len(ys), ys, s=14, alpha=0.6,
                           color=PALETTE[i % len(PALETTE)])
            ax.set_xticks(range(len(set(_vals(vmap, cats[0])))))
            ax.set_xticklabels(sorted(set(_vals(vmap, cats[0]))), fontsize=7,
                               rotation=30, ha="right")
            _style(ax, cats[0], k); sub = f"strip ({cats[0]} × {k})"
        else:
            ax.hist(_vals(vmap, k), bins=20, color=PALETTE[0], alpha=0.85)
            _style(ax, k, "count"); sub = "histogram (realistic range)"
    elif len(numeric) == 2:
        fig, ax = plt.subplots(figsize=(6.6, 5.8))
        _scatter(ax, _vals(vmap, numeric[0]), _vals(vmap, numeric[1]),
                 numeric[0], numeric[1], color_vals)
        sub = "scatter"
    elif len(numeric) > 2:
        pairs = list(combinations(numeric, 2))
        cols = min(3, len(pairs)); rows = (len(pairs) + cols - 1) // cols
        fig, axes = plt.subplots(rows, cols, figsize=(cols * 3.7, rows * 3.4),
                                 squeeze=False)
        for ax, (a, b) in zip(axes.ravel(), pairs):
            _scatter(ax, _vals(vmap, a), _vals(vmap, b), a, b, color_vals)
            if ax.get_legend():
                ax.get_legend().remove()
        for ax in axes.ravel()[len(pairs):]:
            ax.axis("off")
        sub = f"projection matrix ({len(numeric)} numeric vars, {len(pairs)} pairs)"
    else:
        fig, ax = plt.subplots(figsize=(6, 3))
        ax.axis("off")
        ax.text(0.5, 0.5, "no renderable numeric/enum interface", ha="center",
                color=MUTED)
        sub = "—"

    fig.suptitle(title, fontsize=13, color=INK, weight="bold", x=0.02, ha="left")
    fig.text(0.02, 0.94, f"{sub}   ·   {len(samples)} samples of the interface",
             fontsize=9, color=MUTED)
    fig.tight_layout(rect=(0, 0, 1, 0.93))
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
    fig = render(samples, vmap, ann, title)
    os.makedirs(outdir, exist_ok=True)
    path = os.path.join(outdir, f"{base}__{schema}.png")
    fig.savefig(path, dpi=120, facecolor="white"); plt.close(fig)
    return path


if __name__ == "__main__":   # quick single-schema test
    ev, sch = sys.argv[1], sys.argv[2]
    src = open(os.path.join(ROOT, ev)).read()
    print(generate(ev, sch, src, os.path.join(ROOT, "viz", "diagrams")))
