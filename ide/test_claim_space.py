#!/usr/bin/env python3
"""Test the CATEGORICAL solution-space grid for Seq/enum claims (#136).

A raw claim has no run, so `claim_space` is the only static view of its solution set. For NUMERIC
claims it draws z3-Optimize bounds + a per-cell feasible region. For the Seq(Int)/enum/board claims
a verification engineer most wants to SEE — N-queens, sudoku, graph-coloring, toposort — the right
view is a per-POSITION feasibility grid: for each sequence position i (or board cell / region) and
each candidate value v, z3-check SAT of `body ∧ seq[i] == v` → "can this position take this value,
consistent with the rest of the claim?". Rows = positions, cols = candidate values, cell shaded
feasible vs infeasible — the categorical analog of the numeric per-cell solve.

The regression this guards: claim_space used to DEGRADE to a bare N/A card ("this claim has no
numeric variable to bound") for exactly these claims, so the only interrogation move was a single
⊨ Solve witness. This pins BOTH directions:

  - A Seq(Int)/enum claim now yields a REAL grid — non-empty `mask` with ≥1 feasible AND ≥1
    infeasible cell (a flat all-feasible grid would be no information). N-queens 4×4: each
    queen-row CAN occupy only some columns; the corners are infeasible.
  - A genuinely structure-free claim (only scalars, no Seq/enum/board) still returns None — the
    honest N/A. We must not fabricate a grid where there's nothing categorical to show.

`render_claim_space.categorical_grid(smt2, schema)` is the test seam: it computes the grid (no
drawing) and returns the `{rows, cols, mask, …}` dict, or None for the real N/A case.

Run from repo root: `python3 ide/test_claim_space.py` (exit non-zero on any failure)."""
import sys
import tempfile

sys.path.insert(0, "ide/web")
sys.path.insert(0, "viz")

from runtime_io import _export                                 # noqa: E402
import render_claim_space as RC                                # noqa: E402

# N-QUEENS as constraints: col[i] is the column of the queen in row i. The feasibility grid is the
# classic 4-queens picture — row 0 cannot be in column 0 or 3 (no solution places a corner queen).
QUEENS = (
    "claim queens\n"
    "    col ∈ Seq(Int)\n"
    "    #col = 4\n"
    "    ∀ i ∈ {0..3} :\n"
    "        0 ≤ col[i]\n"
    "        col[i] ≤ 3\n"
    "    ∀ i ∈ {0..3} :\n"
    "        ∀ j ∈ {0..3} :\n"
    "            i < j ⇒\n"
    "                col[i] ≠ col[j]\n"
    "                col[i] - col[j] ≠ i - j\n"
    "                col[i] - col[j] ≠ j - i\n")

# GRAPH-COLORING: six regions, enum-typed, adjacency constraints. Every region CAN take every colour
# in SOME solution (the graph is 3-colorable with full colour symmetry) → an all-feasible enum grid.
COLORING = (
    "enum Hue = Red | Green | Blue\n"
    "claim graph_coloring\n"
    "    wa  ∈ Hue\n"
    "    nt  ∈ Hue\n"
    "    sa  ∈ Hue\n"
    "    q   ∈ Hue\n"
    "    nsw ∈ Hue\n"
    "    v   ∈ Hue\n"
    "    wa ≠ nt\n"
    "    wa ≠ sa\n"
    "    nt ≠ sa\n"
    "    nt ≠ q\n"
    "    sa ≠ q\n"
    "    sa ≠ nsw\n"
    "    sa ≠ v\n"
    "    q  ≠ nsw\n"
    "    nsw ≠ v\n")

# SCALAR-ONLY claim: no Seq, no enum, no board — there is genuinely nothing categorical to show, so
# categorical_grid must return None (the honest N/A), NOT a fabricated grid.
SCALAR = (
    "claim pair\n"
    "    x ∈ Int\n"
    "    y ∈ Int\n"
    "    x + y = 7\n")


def _grid(src, work):
    ok, prefix, _dropped, msg = _export(src, work)
    if not ok:
        return None, f"export failed: {msg.splitlines()[0][:80] if msg else ''}"
    g = RC.categorical_grid(prefix + ".smt2", prefix + ".schema.json")
    return g, None


def main():
    fails = []

    # ── QUEENS: a REAL grid with both feasible AND infeasible cells (information, not a flat sheet) ──
    with tempfile.TemporaryDirectory() as work:
        g, err = _grid(QUEENS, work)
        if err:
            fails.append(f"queens: {err}")
        elif g is None:
            fails.append("queens: got the N/A card (None) — a Seq(Int) claim must render a real grid")
        else:
            mask = g["mask"]
            cells = [c for row in mask for c in row]
            if not cells:
                fails.append("queens: empty grid")
            elif not any(cells):
                fails.append("queens: grid has NO feasible cell — claim is solvable, so this is wrong")
            elif all(cells):
                fails.append("queens: grid is ALL feasible — the corner queens must be infeasible "
                             "(an all-shaded grid carries no information)")
            elif len(mask) != 4 or any(len(r) != 4 for r in mask):
                fails.append(f"queens: expected a 4×4 grid (4 rows × cols 0..3), got "
                             f"{len(mask)}×{len(mask[0]) if mask else 0}")

    # ── COLORING: a REAL enum grid, every cell feasible (3-colorable with colour symmetry) ──
    with tempfile.TemporaryDirectory() as work:
        g, err = _grid(COLORING, work)
        if err:
            fails.append(f"coloring: {err}")
        elif g is None:
            fails.append("coloring: got the N/A card (None) — an enum claim must render a real grid")
        else:
            mask = g["mask"]
            cells = [c for row in mask for c in row]
            if not cells:
                fails.append("coloring: empty grid")
            elif not all(cells):
                fails.append("coloring: every region CAN take every colour (symmetric 3-coloring) — "
                             f"expected all cells feasible, got {sum(cells)}/{len(cells)}")
            elif len(mask) != 6 or any(len(r) != 3 for r in mask):
                fails.append(f"coloring: expected 6 regions × 3 colours, got "
                             f"{len(mask)}×{len(mask[0]) if mask else 0}")

    # ── SCALAR-ONLY: honest N/A — categorical_grid returns None, no fabricated grid ──
    with tempfile.TemporaryDirectory() as work:
        g, err = _grid(SCALAR, work)
        if err:
            fails.append(f"scalar: {err}")
        elif g is not None:
            fails.append("scalar: a scalar-only claim has NO categorical structure — "
                         f"categorical_grid must return None (the honest N/A), got {list(g)!r}")

    if fails:
        print("CLAIM-SPACE GRID TEST FAILURES:")
        for f in fails:
            print("  ✗", f)
        return 1
    print("✓ claim-space grid: N-queens renders a real 4×4 feasibility grid (corner queens "
          "infeasible), graph-coloring a 6×3 all-feasible enum grid; a scalar-only claim stays "
          "an honest N/A (no fabricated grid)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
