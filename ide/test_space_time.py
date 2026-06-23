#!/usr/bin/env python3
"""Test the space_time view (#308): a Seq-carried-state FSM's evolution as a 2D raster.

space_time stacks a Seq-carried FSM's ticks into one image — rows = ticks, columns = Seq
positions, color = cell value. For Rule 90 seeded with a single 1, the raster IS the
Sierpiński triangle. This is renderer-INDEPENDENT: it asserts on the simulated GRID (the
data the imshow draws), not the PNG, so it stays green whatever matplotlib does.

Pins:
  - the view is REGISTERED (in render.RENDERERS / VIEWS) and finds the Seq-carried var.
  - Rule 90: the raster shape is (#ticks × 11), and tick-1 is the EXACT neighbour-XOR of
    the single-seed tick-0 row — so the fractal is the real rule applied, not decoration.
    A spot-check of a later tick confirms the triangle keeps expanding by the rule.
  - the raster is BINARY for Rule 90 (0/1 only — the crisp two-color CA case).
  - a diffusion buffer (Int-valued Seq, range of magnitudes) yields a NON-binary grid —
    the colormap path — so the view is GENERIC, not CA-specific.
  - a scalar-only FSM (no Seq) produces no raster (the view's N/A guard fires).

Run from repo root: `python3 ide/test_space_time.py` (exit non-zero on any failure).
"""
import os
import sys
import tempfile

sys.path.insert(0, "ide/web")
sys.path.insert(0, "viz")

from runtime_io import _export                              # noqa: E402
from evident_viz import load as load_model                 # noqa: E402
from render import RENDERERS, VIEWS                         # noqa: E402
import render_space_time as ST                              # noqa: E402

RULE90 = (
    "fsm rule90\n"
    "    cells ∈ Seq(Int)\n"
    "    #cells = 11\n"
    "    is_first_tick ⇒ cells = ⟨0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0⟩\n"
    "    ∀ i ∈ {1..9} : cells[i] = ((_cells[i-1] + _cells[i+1]) = 1 ? 1 : 0)\n"
    "    (cells[0] = 0 ∧ cells[10] = 0)\n")

# A 1-D box-blur diffusion buffer: a single spike of 64 smooths outward, so cells take a
# RANGE of integer magnitudes (not just 0/1) — exercises the colormap (non-binary) path.
DIFFUSE = (
    "fsm diffuse\n"
    "    buf ∈ Seq(Int)\n"
    "    #buf = 9\n"
    "    is_first_tick ⇒ buf = ⟨0, 0, 0, 0, 64, 0, 0, 0, 0⟩\n"
    "    ∀ i ∈ {1..7} : buf[i] = (_buf[i-1] + 2 * _buf[i] + _buf[i+1]) / 4\n"
    "    (buf[0] = 0 ∧ buf[8] = 0)\n")

# A scalar-only FSM: no Seq-carried state, so space_time is N/A (the guard fires).
SCALAR = (
    "fsm counter\n"
    "    count ∈ Int\n"
    "    is_first_tick ⇒ count = 0\n"
    "    ¬is_first_tick ⇒ Δcount = (_count < 5 ? 1 : 0)\n")


def _model(src, work):
    os.makedirs(work, exist_ok=True)
    ok, prefix, dropped, msg = _export(src, work)
    assert ok, f"export failed: {(msg or '')[:80]}"
    assert dropped == 0, f"dropped {dropped} constraints"
    return load_model(prefix + ".smt2", prefix + ".schema.json"), prefix


def _xor_row(prev):
    """Rule 90 reference: cell i becomes (prev[i-1] XOR prev[i+1]); the two ends are 0."""
    n = len(prev)
    out = []
    for i in range(n):
        if i == 0 or i == n - 1:
            out.append(0)
        else:
            out.append(1 if (prev[i - 1] + prev[i + 1]) == 1 else 0)
    return out


def _check_rule90(work, fails):
    """Rule 90 is the headline Sierpiński case: deterministic, 11 cells wide, and every
    tick is the exact neighbour-XOR of the one above — so the raster IS the rule applied."""
    m, _ = _model(RULE90, os.path.join(work, "r90"))
    seq = ST._seq_var(m)
    if seq is None or seq["name"] != "cells":
        fails.append(f"space_time didn't find the Seq-carried var (got {seq})")
        return
    rows, nondet, _ = ST._simulate(m, "cells")
    if not rows:
        fails.append("Rule 90 produced no raster rows")
        return
    if {len(r) for r in rows} != {11}:
        fails.append(f"Rule 90 raster not 11 cells wide: {[len(r) for r in rows]}")
    if len(rows) < 5:
        fails.append(f"Rule 90 raster too short: {len(rows)} ticks")
    if nondet:
        fails.append("Rule 90 should be DETERMINISTIC (one successor/tick)")
    seed = [0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0]
    if rows[0] != seed:
        fails.append(f"Rule 90 tick-0 != single seed: {rows[0]}")
    expected1 = _xor_row(seed)               # {…,0,1,0,1,0,…} — the seed's two neighbours
    if rows[1] != expected1:
        fails.append(f"Rule 90 tick-1 XOR mismatch: got {rows[1]}, expected {expected1}")
    # every row is the XOR of the row above — the fractal is the rule, not a coincidence
    for t in range(1, len(rows)):
        if rows[t] != _xor_row(rows[t - 1]):
            fails.append(f"Rule 90 tick-{t} is not the XOR of tick-{t-1}")
            break
    grid, _, _ = ST._numify(rows, "int", m, "cells")
    if not ST._is_binary(grid):
        fails.append("Rule 90 grid should be binary (0/1 only)")


def _check_diffuse(work, fails):
    """A diffusion buffer's cells take a RANGE of magnitudes → the NON-binary colormap
    path; proves the view is generic over Seq(Int), not hardwired to a 0/1 CA."""
    md, _ = _model(DIFFUSE, os.path.join(work, "diff"))
    seqd = ST._seq_var(md)
    if seqd is None or seqd["name"] != "buf":
        fails.append(f"diffuse didn't find the Seq-carried var (got {seqd})")
        return
    drows, _, _ = ST._simulate(md, "buf")
    if not drows:
        fails.append("diffuse produced no raster rows")
        return
    dgrid, _, _ = ST._numify(drows, "int", md, "buf")
    if ST._is_binary(dgrid):
        fails.append("diffuse grid should be NON-binary (a range of values → colormap path)")
    vals = {c for r in dgrid for c in r}
    if max(vals) <= 1:
        fails.append(f"diffuse never exceeded 1 — no magnitude to color: {sorted(vals)}")


def main():
    fails = []

    if "space_time" not in RENDERERS or "space_time" not in VIEWS:
        fails.append("space_time is not registered in render.RENDERERS/VIEWS")

    with tempfile.TemporaryDirectory() as work:
        _check_rule90(work, fails)
        _check_diffuse(work, fails)
        # a scalar-only FSM carries no Seq → space_time is N/A (the guard fires)
        ms, _ = _model(SCALAR, os.path.join(work, "scal"))
        if ST._seq_var(ms) is not None:
            fails.append("scalar FSM wrongly reported a Seq-carried var for space_time")

    if fails:
        print("SPACE_TIME TEST FAILURES:")
        for f in fails:
            print("  ✗", f)
        return 1
    print("✓ space_time: Rule 90 raster is the Sierpiński XOR triangle (11 cells, binary); "
          "diffusion buffer takes the colormap path; scalar FSM is N/A")
    return 0


if __name__ == "__main__":
    sys.exit(main())
