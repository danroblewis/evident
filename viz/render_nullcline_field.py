#!/usr/bin/env python3
"""render_nullcline_field — qualitative-flow / nullcline visualization of an
Evident program's transition relation.

For a 2-variable NUMERIC system the plane is shaded by the SIGN of each
component's change over a grid:

    dx = successor.x - x        dv = successor.v - v

This partitions the plane into (up to) four sign-regions, and we overlay the
approximate NULLCLINES — the curves where dx ~ 0 (x-nullcline) and dv ~ 0
(v-nullcline). Their intersections are the fixed points; the sign-regions tell
you which way the flow turns through each quadrant. This is the standard
qualitative phase-plane analysis, read straight off the transition.

The dynamics come ENTIRELY from querying m.successor(...) on a grid of pinned
seed points — nothing is hardcoded.

Discrete / non-2-numeric programs get a clear titled placeholder (the analysis
is undefined without a continuous 2D state plane). A projection note explains
why.

Usage:
    python3 viz/render_nullcline_field.py <smt2> <schema> <out_path>
"""
import sys
import os

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.colors import ListedColormap
import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from evident_viz import load

VIZ = "nullcline_field"
GRID = 41            # samples per axis
PAD = 1.10           # extent padding beyond the seed-derived range


def numeric_vars(m):
    return [v for v in m.state_vars if v["kind"] in ("int", "real")]


def placeholder(m, out_path, reason):
    fig, ax = plt.subplots(figsize=(8, 6))
    ax.axis("off")
    ax.text(0.5, 0.62, f"N/A for this state: {reason}",
            ha="center", va="center", fontsize=14, wrap=True,
            transform=ax.transAxes)
    kinds = ", ".join(f"{v['name']}:{v['kind']}" for v in m.state_vars)
    ax.text(0.5, 0.42, f"state = [{kinds}]", ha="center", va="center",
            fontsize=10, color="#555", transform=ax.transAxes)
    ax.text(0.5, 0.30,
            "nullcline_field needs a 2-D numeric state plane\n"
            "(sign of dx, dv requires continuous x and v axes).",
            ha="center", va="center", fontsize=10, color="#777",
            transform=ax.transAxes)
    ax.set_title(f"{m.fsm} — {VIZ}", fontsize=13, fontweight="bold")
    fig.tight_layout()
    fig.savefig(out_path, dpi=120)
    plt.close(fig)


def axis_extent(m, xv, vv):
    """Derive a plotting window. Prefer the natural fixed-point scale: probe a
    few seeds to find a working range; fall back to a generous default."""
    # Try a wide grid of probe points; keep ones the transition accepts.
    probes = [-3200, -1600, -800, -400, 0, 400, 800, 1600, 3200]
    xs, vs = [], []
    for px in probes:
        for pv in probes:
            st = {xv["name"]: px, vv["name"]: pv}
            nxt = m.successor(st)
            if nxt is not None:
                xs.append(px); vs.append(pv)
                xs.append(nxt[xv["name"]]); vs.append(nxt[vv["name"]])
    if xs:
        xlo, xhi = min(xs), max(xs)
        vlo, vhi = min(vs), max(vs)
    else:
        xlo, xhi, vlo, vhi = -3200, 3200, -3200, 3200
    # symmetric-ish padding
    def pad(lo, hi):
        c = 0.5 * (lo + hi)
        r = max(1.0, 0.5 * (hi - lo)) * PAD
        return c - r, c + r
    xlo, xhi = pad(xlo, xhi)
    vlo, vhi = pad(vlo, vhi)
    return xlo, xhi, vlo, vhi


def render_numeric(m, out_path, xv, vv):
    xlo, xhi, vlo, vhi = axis_extent(m, xv, vv)
    xs = np.linspace(xlo, xhi, GRID)
    vs = np.linspace(vlo, vhi, GRID)
    DX = np.full((GRID, GRID), np.nan)   # rows = v index, cols = x index
    DV = np.full((GRID, GRID), np.nan)
    for j, vval in enumerate(vs):
        for i, xval in enumerate(xs):
            st = {xv["name"]: int(round(xval)), vv["name"]: int(round(vval))}
            nxt = m.successor(st)
            if nxt is None:
                continue
            DX[j, i] = nxt[xv["name"]] - st[xv["name"]]
            DV[j, i] = nxt[vv["name"]] - st[vv["name"]]

    # Four sign-regions: encode (sign dx, sign dv) -> 0..3
    sdx = np.sign(DX)
    sdv = np.sign(DV)
    region = np.full((GRID, GRID), np.nan)
    # 0: dx<0,dv<0  1: dx>=0,dv<0  2: dx<0,dv>=0  3: dx>=0,dv>=0
    mask = ~np.isnan(DX)
    region[mask] = (sdx[mask] >= 0).astype(float) + 2 * (sdv[mask] >= 0).astype(float)

    fig, ax = plt.subplots(figsize=(9, 7.5))
    # soft four-region shading
    cmap = ListedColormap(["#cfe8ff", "#ffe0cc", "#d8f0d0", "#f3d6ec"])
    ax.imshow(region, origin="lower", extent=[xlo, xhi, vlo, vhi],
              aspect="auto", cmap=cmap, vmin=-0.5, vmax=3.5, alpha=0.85,
              interpolation="nearest")

    # Nullclines: zero level-sets of DX and DV.
    X, V = np.meshgrid(xs, vs)
    try:
        cx = ax.contour(X, V, DX, levels=[0], colors="#1f4fff",
                        linewidths=2.4)
        ax.clabel(cx, fmt={0: f"d{_short(xv)}=0"}, fontsize=9)
    except Exception:
        pass
    try:
        cv = ax.contour(X, V, DV, levels=[0], colors="#d62728",
                        linewidths=2.4)
        ax.clabel(cv, fmt={0: f"d{_short(vv)}=0"}, fontsize=9)
    except Exception:
        pass

    # A thin flow-direction quiver to make the four regions legible.
    step = max(1, GRID // 18)
    Xq = X[::step, ::step]; Vq = V[::step, ::step]
    U = DX[::step, ::step]; W = DV[::step, ::step]
    mag = np.hypot(U, W)
    with np.errstate(invalid="ignore", divide="ignore"):
        Un = np.where(mag > 0, U / mag, 0)
        Wn = np.where(mag > 0, W / mag, 0)
    ax.quiver(Xq, Vq, Un, Wn, color="#444", alpha=0.55,
              scale=32, width=0.0026, pivot="mid")

    # Mark fixed points: grid cells where both dx and dv are ~0.
    near0 = mask & (np.abs(DX) <= _tol(DX)) & (np.abs(DV) <= _tol(DV))
    if near0.any():
        fpx = X[near0]; fpv = V[near0]
        ax.scatter(fpx, fpv, s=70, facecolor="black", edgecolor="white",
                   zorder=6, label="≈ fixed point")

    ax.set_xlim(xlo, xhi); ax.set_ylim(vlo, vhi)
    ax.set_xlabel(_short(xv)); ax.set_ylabel(_short(vv))
    ax.set_title(f"{m.fsm} — {VIZ}\nsign-regions of (d{_short(xv)}, d{_short(vv)}) "
                 f"+ nullclines", fontsize=13, fontweight="bold")

    # Region legend
    from matplotlib.patches import Patch
    leg = [
        Patch(facecolor="#f3d6ec", label="d{0}↑ d{1}↑".format(_short(xv), _short(vv))),
        Patch(facecolor="#d8f0d0", label="d{0}↓ d{1}↑".format(_short(xv), _short(vv))),
        Patch(facecolor="#ffe0cc", label="d{0}↑ d{1}↓".format(_short(xv), _short(vv))),
        Patch(facecolor="#cfe8ff", label="d{0}↓ d{1}↓".format(_short(xv), _short(vv))),
    ]
    ax.legend(handles=leg, loc="upper right", fontsize=8, framealpha=0.9)
    ax.grid(alpha=0.15, linewidth=0.5)
    fig.tight_layout()
    fig.savefig(out_path, dpi=120)
    plt.close(fig)


def _tol(arr):
    finite = arr[~np.isnan(arr)]
    if finite.size == 0:
        return 0.0
    # "near zero" relative to typical step magnitude
    scale = np.percentile(np.abs(finite), 60)
    return max(1.0, 0.12 * scale)


def _short(v):
    n = v["name"]
    return n.split(".")[-1]


def main():
    if len(sys.argv) != 4:
        print("usage: render_nullcline_field.py <smt2> <schema> <out_path>",
              file=sys.stderr)
        sys.exit(2)
    smt2, schema, out = sys.argv[1], sys.argv[2], sys.argv[3]
    m = load(smt2, schema)

    nums = numeric_vars(m)
    if len(nums) == 2 and all(v["kind"] in ("int", "real") for v in m.state_vars):
        render_numeric(m, out, nums[0], nums[1])
    elif len(nums) == 2:
        # 2 numeric vars but extra discrete vars present — still do the plane,
        # projecting away the discrete ones (successor picks some value for them).
        render_numeric(m, out, nums[0], nums[1])
    else:
        if m.is_discrete():
            reason = "purely discrete state (no continuous axes)"
        else:
            reason = (f"{len(nums)} numeric var(s); need exactly 2 for a "
                      f"qualitative-flow plane")
        placeholder(m, out, reason)
    print(f"wrote {out}")


if __name__ == "__main__":
    main()
