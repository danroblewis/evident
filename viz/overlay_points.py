#!/usr/bin/env python3
"""overlay_points.py — the interactive hover-overlay sidecar, shared across the
point-plotting renderers (#184).

Every view that plots states as POINTS (solution_space's reachable cloud,
state_graph's nodes, phase_portrait's orbit, orbit_scatter's trajectory,
basin_map's colored dots) emits a `<out>.points.json` sidecar: a list of
`{fx, fy, state}` where fx/fy are the point's FRACTIONAL position within the
SAVED image (top-left origin, both 0..1) and state is the point's full
assignment. The server reads it GENERICALLY for any view; the frontend overlays
a transparent target at fx*100%/fy*100% — hover → state tooltip, click → pin.

Two save modes need two mappings (getting this wrong drifts the markers off the
dots):
  * PLAIN  `fig.savefig(out)` — fractions are taken against the whole figure via
    `transFigure` (solution_space). Use `figure_fraction`.
  * TIGHT  `fig.savefig(out, bbox_inches="tight")` — matplotlib crops to a tight
    bbox, so fractions must be RELATIVE TO THAT CROP, not the full figure
    (state_graph). Use `tight_fraction`, which reads the same tight bbox savefig
    will use.

Both flip y to a top-left origin (matching the frontend wrapper) and keep only
ON-CANVAS points. `OVERLAY_CAP` bounds the emitted targets so a several-hundred
point picture doesn't paint hundreds of hit-zones.
"""
import json

OVERLAY_CAP = 60

_SHORT = lambda n: n.split(".")[-1]


def write_points(out_path, points):
    """Write the `<out>.points.json` sidecar. Empty list → the overlay no-ops.
    Never raises — a sidecar failure must not fail the render."""
    try:
        with open(out_path + ".points.json", "w") as f:
            json.dump(points, f)
    except Exception:
        pass


def _emit(ffx, ffy, state, out):
    if 0.0 <= ffx <= 1.0 and 0.0 <= ffy <= 1.0:
        out.append({"fx": round(float(ffx), 4),
                    "fy": round(float(1.0 - ffy), 4),
                    "state": {_SHORT(k): v for k, v in state.items()}})


def figure_fraction(fig, entries):
    """PLAIN-save mapping (figure-relative). `entries` = iterable of
    (ax, data_x, data_y, state_dict). Returns up to OVERLAY_CAP points."""
    fig.canvas.draw()
    inv = fig.transFigure.inverted()
    points = []
    for ax, dx, dy, st in entries:
        disp = ax.transData.transform((dx, dy))
        ffx, ffy = inv.transform(disp)
        _emit(ffx, ffy, st, points)
        if len(points) >= OVERLAY_CAP:
            break
    return points


def tight_fraction(fig, entries):
    """TIGHT-save mapping (crop-relative). Reads the same tight bbox savefig will
    use and normalizes within THAT. `entries` = iterable of
    (ax, data_x, data_y, state_dict). Returns up to OVERLAY_CAP points."""
    fig.canvas.draw()
    dpi = fig.dpi
    tb = fig.get_tightbbox(fig.canvas.get_renderer())
    x0, y0 = tb.x0 * dpi, tb.y0 * dpi
    w, h = tb.width * dpi, tb.height * dpi
    if w <= 0 or h <= 0:
        return []
    points = []
    for ax, dx, dy, st in entries:
        px, py = ax.transData.transform((dx, dy))
        ffx = (px - x0) / w
        ffy = (py - y0) / h
        _emit(ffx, ffy, st, points)
        if len(points) >= OVERLAY_CAP:
            break
    return points
