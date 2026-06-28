"""occupancy_data.py — the ABSTRACT `<out>.data.json` substrate for render_occupancy_heatmap.

The PNG is a density picture; this is its MEANING in machine-checkable form. A golden test asserts on
THIS, not on pixels: it captures the occupancy grid the heatmap rasters — the bin centers, the per-cell
visit counts, the peak cell, the per-axis spread, the total sample mass — so an expert can check it
against the model's true occupancy mathematics (e.g. a 2-D random walk: peaked at the origin, symmetric
about it, variance growing in time).

Schema (`<out>.data.json`):

    {
      "view":   "occupancy_heatmap",
      "model":  "<fsm name>",
      "status": "ok" | "na" | "empty",     # "na"/"empty" → no grid (an honest N/A / no states)
      "na_reason": "<reason>"|null,         # set when status != "ok"
      "axes":   {"x": "<short>", "y": "<short>"},
      "grid": {                             # present only when status == "ok"
        "x_centers": [...], "y_centers": [...],   # bin centers along each axis
        "counts": [[...], ...],             # counts[ix][iy] — raw (un-logged) visit mass
        "nx": int, "ny": int
      },
      "peak":   {"x": v, "y": v, "count": n}|null,   # the densest cell (where it dwells most)
      "total":  int,                        # total points binned (sample mass)
      "spread": {"x": float, "y": float},   # std-dev of the binned points per axis (diffusion width)
      "mean":   {"x": float, "y": float}     # centroid of the binned points (should sit at the peak)
    }

Built from the SAME (xs, ys) + extent + bin edges the renderer histograms, so the data can never
disagree with the picture.
"""
import json

import numpy as np

from render_common import short


def _peak(x_centers, y_centers, counts):
    if counts.size == 0 or counts.max() <= 0:
        return None
    ix, iy = np.unravel_index(int(np.argmax(counts)), counts.shape)
    return {"x": float(x_centers[ix]), "y": float(y_centers[iy]),
            "count": int(counts[ix, iy])}


def build_grid(m, a0, a1, xs, ys, xedges, yedges, counts):
    """The status='ok' data dict from the histogram the renderer drew: `counts` is the raw (NOT log)
    2-D histogram over `xedges`×`yedges`; `xs`/`ys` are the binned points (for spread/mean)."""
    xc = ((xedges[:-1] + xedges[1:]) / 2.0).tolist()
    yc = ((yedges[:-1] + yedges[1:]) / 2.0).tolist()
    counts = np.asarray(counts, float)
    xs = np.asarray(xs, float); ys = np.asarray(ys, float)
    return {
        "view": "occupancy_heatmap",
        "model": m.fsm,
        "status": "ok",
        "na_reason": None,
        "axes": {"x": short(a0["name"]), "y": short(a1["name"])},
        "grid": {"x_centers": xc, "y_centers": yc,
                 "counts": counts.tolist(), "nx": len(xc), "ny": len(yc)},
        "peak": _peak(np.array(xc), np.array(yc), counts),
        "total": int(len(xs)),
        "spread": {"x": float(np.std(xs)) if len(xs) else 0.0,
                   "y": float(np.std(ys)) if len(ys) else 0.0},
        "mean": {"x": float(np.mean(xs)) if len(xs) else 0.0,
                 "y": float(np.mean(ys)) if len(ys) else 0.0},
    }


def na(m, a0, a1, reason, status="na"):
    """An honest no-grid record (N/A card, empty/degenerate reachable set). Axes may be None."""
    return {
        "view": "occupancy_heatmap",
        "model": m.fsm,
        "status": status,
        "na_reason": reason,
        "axes": {"x": short(a0["name"]) if a0 else None,
                 "y": short(a1["name"]) if a1 else None},
        "grid": None, "peak": None, "total": 0,
        "spread": None, "mean": None,
    }


def write(out_path, data):
    """Write `<out>.data.json`. Mirrors region_data.write / overlay_points.write_points: NEVER raises
    — a sidecar failure must not fail the render."""
    try:
        with open(out_path + ".data.json", "w") as f:
            json.dump(data, f)
    except Exception:
        pass
