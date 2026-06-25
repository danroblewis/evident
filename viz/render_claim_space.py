"""claim_space — the solution space of a raw CLAIM (a static constraint, not an FSM).

A claim has NO runs — so this view is entirely SOLVED, never sampled. Two shapes:

NUMERIC (≥2 numeric vars) — the original feasible-region view:
  * left  — each numeric variable's EXACT range over the solution set, via z3 Optimize
            (a provable sup/inf of the variable subject to the whole constraint).
  * right — the real FEASIBLE REGION of the two principal variables: we grid value-space and
            ask the solver, per cell, "is (x, y) extensible to a full satisfying assignment?"
            — shading the cells that are, so the boundary is the true edge of the solution set
            (not a bounding box).

CATEGORICAL (Seq(Int)/enum-shaped claims — queens, graph-coloring, toposort, sudoku) — a
feasibility grid solved per cell, the discrete analog of the numeric per-cell solve:
  * enum vars   → rows = vars, cols = variants; cell shaded iff `var == variant` is sat.
  * one Seq(Int) of length N → N × value-range grid; cell (i, v) shaded iff `seq[i] == v` is sat.
  * record-element Seq (Seq(Edge)) → one row per (index, numeric field).
`_categorical(...)` computes the grid dict (`{rows, cols, mask, …}` or `{"na": …}`) and `_draw_grid`
renders it; `categorical_grid(smt2, schema)` is the headless seam (returns the dict, or None for a
genuinely structure-free claim) that `ide/test_claim_space.py` asserts on.

Consumes the `{"claim", "vars":[{name,kind,role,...}]}` schema `evident export` emits: scalars
carry just kind; a `seq` var also carries `elem` + (if pinned) `len`; an `enum` var carries
`variants`. Record-element Seqs (Seq(Edge)) aren't listed and are skipped.
"""
import json
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
import z3

_SHORT = lambda n: n.split(".")[-1]


def _num(z):
    try:
        return z.as_long()
    except Exception:
        pass
    try:
        return round(float(z.as_fraction()), 3)
    except Exception:
        return None


def _load_claim(smt2_path, schema_path):
    sch = json.load(open(schema_path))
    assertions = z3.parse_smt2_file(smt2_path)
    consts, seen = {}, set()

    def walk(e):
        if e.get_id() in seen:
            return
        seen.add(e.get_id())
        if z3.is_const(e) and e.decl().kind() == z3.Z3_OP_UNINTERPRETED:
            consts[e.decl().name()] = e
        for ch in e.children():
            walk(ch)

    for a in assertions:
        walk(a)
    body = z3.And(*assertions) if len(assertions) != 1 else assertions[0]
    return sch, body, consts


def _na(out_path, title, msg):
    fig, ax = plt.subplots(figsize=(9, 6))
    ax.text(0.5, 0.5, msg, ha="center", va="center", transform=ax.transAxes, fontsize=13)
    ax.set_xticks([]); ax.set_yticks([])
    ax.set_title(title, fontsize=13)
    fig.tight_layout(); fig.savefig(out_path, dpi=120); plt.close(fig)
    return out_path


def _opt_bound(body, c, maximize):
    """Exact sup/inf of constant c over the constraint, or None if UNBOUNDED/unsat. Reads the bound
    off the objective HANDLE (upper/lower), NOT the witness model: an unbounded objective returns sat
    with a finite witness while upper/lower = ±∞, so reading the witness fabricated a false finite
    bound (a≥3 → 3, labelled 'exact'). _finite_numeric returns None for the ±∞ sentinel."""
    from model_const import SOLVE_TIMEOUT_MS
    from model_global import _finite_numeric
    o = z3.Optimize()
    o.set("timeout", SOLVE_TIMEOUT_MS)                     # bounded: a nonlinear Optimize times out → None
    o.add(body)
    h = o.maximize(c) if maximize else o.minimize(c)
    if o.check() != z3.sat:
        return None
    return _finite_numeric(o.upper(h) if maximize else o.lower(h))


def _ctor_by_name(sort, vname):
    """The nullary constructor of an enum sort whose name is `vname`, or None."""
    for k in range(sort.num_constructors()):
        d = sort.constructor(k)
        if d.name() == vname:
            return d
    return None


def _grid(out_path, name, title, row_labels, col_labels, mask, xlabel, ylabel):
    """Shared categorical-feasibility heatmap: a row × col grid, cell shaded iff
    mask[r][c] (the value is feasible in SOME satisfying assignment)."""
    fig, ax = plt.subplots(figsize=(max(7, 0.8 * len(col_labels) + 3),
                                    max(4.5, 0.55 * len(row_labels) + 2)))
    grid = np.array(mask, dtype=float)
    ax.imshow(grid, aspect="auto", cmap="Greens", vmin=0, vmax=1,
              interpolation="nearest", alpha=0.85)
    ax.set_xticks(range(len(col_labels))); ax.set_xticklabels(col_labels, fontsize=9)
    ax.set_yticks(range(len(row_labels))); ax.set_yticklabels(row_labels, fontsize=9)
    ax.set_xlabel(xlabel); ax.set_ylabel(ylabel)
    for r in range(len(row_labels)):
        for c in range(len(col_labels)):
            ax.text(c, r, "✓" if grid[r][c] else "·", ha="center", va="center",
                    fontsize=9, color="#0f1419" if grid[r][c] else "#aab1ba")
    ax.set_xticks(np.arange(-0.5, len(col_labels), 1), minor=True)
    ax.set_yticks(np.arange(-0.5, len(row_labels), 1), minor=True)
    ax.grid(which="minor", color="#d0d7de", linewidth=0.6)
    ax.tick_params(which="minor", length=0)
    fig.suptitle(title, fontsize=13)
    fig.tight_layout(rect=[0, 0, 1, 0.95])
    fig.savefig(out_path, dpi=120); plt.close(fig)
    return out_path


def _seq_grid(name, body, consts, seq):
    """Compute (not draw) the per-position Seq feasibility grid. Returns the {rows, cols,
    mask, …} grid dict, or `{"na": msg}` when the Seq is unbounded / has no numeric field
    (no finite grid). A Seq(Int) of length N gives N rows `seq[i]` (queens `col`: squares a
    queen CAN occupy across all solutions). A Seq of records (Seq(Edge), #183) gives one row
    per (index, numeric field) — `edges[i].frm`. The only change is the cell expression:
    `seq[i]` vs `field(seq[i])`."""
    arr = consts[seq["name"]]
    short = _SHORT(seq["name"])
    n = int(seq["len"])
    elem = arr.sort().range()
    if elem.kind() == z3.Z3_DATATYPE_SORT:        # Seq(record) → one row per (index, numeric field)
        fields = [elem.accessor(0, j) for j in range(elem.constructor(0).arity())
                  if elem.accessor(0, j).range().kind() in (z3.Z3_INT_SORT, z3.Z3_REAL_SORT)]
        if not fields:
            return {"na": "the Seq's record has no numeric field to bound"}
        cells = [(i, f) for i in range(n) for f in fields]
        cexpr = (lambda i, f: f(z3.Select(arr, i)))
        rlab = (lambda i, f: f"{short}[{i}].{f.name()}")
        ylabel, what = "index.field", f"{short}[i].field"
    else:                                          # Seq(Int) → one row per index (the original view)
        cells = [(i, None) for i in range(n)]
        cexpr = (lambda i, f: z3.Select(arr, i))
        rlab = (lambda i, f: f"{short}[{i}]")
        ylabel, what = "index", f"{short}[i]"
    # value range: each cell's solved [min, max] (clamped so the grid stays sane)
    lo, hi = None, None
    for i, f in cells:
        emn = _opt_bound(body, cexpr(i, f), maximize=False)
        emx = _opt_bound(body, cexpr(i, f), maximize=True)
        if emn is not None:
            lo = emn if lo is None else min(lo, emn)
        if emx is not None:
            hi = emx if hi is None else max(hi, emx)
    if lo is None or hi is None:
        return {"na": "this claim's Seq values are unbounded\n(no finite feasibility grid)"}
    lo, hi = int(round(lo)), int(round(hi))
    if hi - lo > 40:
        hi = lo + 40
    values = list(range(lo, hi + 1))
    sol = z3.Solver(); sol.add(body)
    mask = []
    for i, f in cells:
        cell = cexpr(i, f)
        row = []
        for vv in values:
            sol.push(); sol.add(cell == vv); ok = sol.check() == z3.sat; sol.pop()
            row.append(1 if ok else 0)
        mask.append(row)
    return {"rows": [rlab(i, f) for i, f in cells], "cols": [str(v) for v in values],
            "mask": mask, "xlabel": "value", "ylabel": ylabel,
            "title": f"{name} — feasible positions (solved per cell)\n"
                     f"shaded = {what} CAN equal this value in some solution"}


def categorical_grid(smt2_path, schema_path):
    """The categorical feasibility grid for a Seq/enum/board claim, computed via z3 (no
    drawing). Returns the {rows, cols, mask, title, …} dict, or None when the claim has no
    Seq/enum/board structure to show (the genuine N/A case). This is the test seam: a real
    grid is `mask` non-empty with ≥1 feasible cell; the N/A card is `None`."""
    sch, body, consts = _load_claim(smt2_path, schema_path)
    name = sch.get("claim", "claim")
    g = _categorical(name, body, consts, sch.get("vars", []))
    return g if (g is not None and "na" not in g) else None


def _categorical(name, body, consts, vars_):
    """Shared Seq/enum/board → grid-dict dispatch used by both `render` and `categorical_grid`.
    Returns a grid dict (possibly `{"na": msg}`) or None if there's no categorical structure.
    enum vars → the variants each region CAN take; Seq(Int) → per-position feasible values;
    record-element Seq (Seq(Edge)) → per-(index, numeric field)."""
    enums = [v for v in vars_ if v.get("kind") == "enum" and v["name"] in consts]
    seqs  = [v for v in vars_ if v.get("kind") == "seq" and v.get("elem") == "int"
             and v["name"] in consts and v.get("len")]
    if enums:
        # rows = enum vars, cols = the shared variants; cell = 1 iff `var == variant` is sat.
        variants = enums[0].get("variants", [])
        sol = z3.Solver(); sol.add(body); mask = []
        for v in enums:
            c, row = consts[v["name"]], []
            for vn in variants:
                ctor = _ctor_by_name(c.sort(), vn)
                ok = False
                if ctor is not None:
                    sol.push(); sol.add(c == ctor()); ok = sol.check() == z3.sat; sol.pop()
                row.append(1 if ok else 0)
            mask.append(row)
        return {"rows": [_SHORT(v["name"]) for v in enums], "cols": list(variants), "mask": mask,
                "xlabel": "value", "ylabel": "variable",
                "title": f"{name} — feasible values (solved per cell)\n"
                         "shaded = the variable CAN take this value in some solution"}
    if seqs:
        return _seq_grid(name, body, consts, seqs[0])
    # record-element Seqs (Seq(Edge)) aren't in the schema vars — detect from the encoding (an
    # `(Array Int <datatype>)` const). The export pins the elements via the ∀ unroll but leaves
    # __len free, so the length is the max literal index the constraints touch via select (#183).
    rec = next((nm for nm, c in consts.items()
                if z3.is_array(c) and c.sort().range().kind() == z3.Z3_DATATYPE_SORT), None)
    if rec is not None:
        arr = consts[rec]; idxs, seen = set(), set()

        def _idx(e):
            if e.get_id() in seen:
                return
            seen.add(e.get_id())
            if z3.is_select(e) and e.arg(0).eq(arr) and z3.is_int_value(e.arg(1)):
                idxs.add(e.arg(1).as_long())
            for ch in e.children():
                _idx(ch)

        _idx(body)
        ln = (max(idxs) + 1) if idxs else 0
        if 0 < ln <= 16:
            return _seq_grid(name, body, consts, {"name": rec, "len": ln})
    return None


def render(smt2_path, schema_path, out_path):
    from z3_budget import _nonlinear
    sch, body, consts = _load_claim(smt2_path, schema_path)
    name = sch.get("claim", "claim")
    if _nonlinear(body):                          # NIA — Z3 Optimize can't soundly bound a product
        return _na(out_path, f"{name} — solution space",
                   "nonlinear claim (a product of variables) — Z3 can't soundly bound it\n"
                   "(nonlinear integer arithmetic is undecidable)")
    vars_ = sch.get("vars", [])
    numeric = [v for v in vars_
               if v.get("kind") in ("int", "real") and v["name"] in consts]

    # exact bounds (z3 Optimize) for every numeric var
    bounds = {}
    for v in numeric:
        c = consts[v["name"]]
        lo = _opt_bound(body, c, maximize=False)
        hi = _opt_bound(body, c, maximize=True)
        bounds[_SHORT(v["name"])] = (lo, hi)
    shown = [n for n in (_SHORT(v["name"]) for v in numeric)
             if None not in bounds[n]]

    # The numeric feasible-region panel needs two solved axes. With fewer, a
    # Seq/enum-shaped claim (queens, graph-coloring, sudoku, toposort) gets a
    # categorical FEASIBILITY view instead — the set of values each variable CAN
    # take in some satisfying assignment, solved per cell. Honest analog of the
    # numeric per-cell solve, for the discrete domains the bounds panel can't show.
    if len(shown) < 2:
        g = _categorical(name, body, consts, vars_)
        if g is not None:                      # a Seq/enum/board claim → its feasibility grid
            if "na" in g:                      # bounded-detection failed (unbounded Seq) → honest N/A
                return _na(out_path, f"{name} — solution space", g["na"])
            return _grid(out_path, name, g["title"], g["rows"], g["cols"], g["mask"],
                         xlabel=g["xlabel"], ylabel=g["ylabel"])
        if not numeric:
            return _na(out_path, f"{name} — solution space",
                       "this claim has no numeric variable to bound\n"
                       "(its variables are Seq/enum — press ⊨ Solve for a witness)")
    if not shown:
        return _na(out_path, f"{name} — solution space",
                   "the claim's numeric variables are unbounded\n(no finite solution-space boundary)")
    return _numeric_view(out_path, name, body, consts, numeric, bounds, shown)


def _numeric_view(out_path, name, body, consts, numeric, bounds, shown):
    """Left: each numeric var's exact solved range. Right (≥2 vars): the real
    feasible region of the two principal vars, solved per cell (not a box)."""
    have2d = len(shown) >= 2
    fig, axes = plt.subplots(1, 2 if have2d else 1, figsize=(14 if have2d else 8.5, 6.5))
    axL = axes[0] if have2d else axes

    # --- left: each variable's EXACT solved range ---
    ys = list(range(len(shown)))
    for y, nm in zip(ys, shown):
        lo, hi = bounds[nm]
        axL.plot([lo, hi], [y, y], lw=9, solid_capstyle="round", color="#58a6ff", alpha=0.5)
        axL.plot([lo, hi], [y, y], "|", color="#0f1419", markersize=16, markeredgewidth=2)
        axL.text(lo, y + 0.2, f"{lo:g}", ha="left", va="bottom", fontsize=9, color="#7d8590")
        axL.text(hi, y + 0.2, f"{hi:g}", ha="right", va="bottom", fontsize=9, color="#7d8590")
    axL.set_yticks(ys); axL.set_yticklabels(shown)
    axL.set_ylim(-0.7, len(shown) - 0.3)
    axL.set_xlabel("value")
    axL.set_title("variable boundaries — exact (z3 Optimize over the constraint)", fontsize=11)
    axL.grid(axis="x", alpha=0.2)

    def _by_short(short):                      # the var dict (full name + kind) for a short name
        return next((v for v in numeric if _SHORT(v["name"]) == short),
                    {"name": short, "kind": "int"})

    # --- right: the real feasible region — per-cell solve, not a bounding box ---
    if have2d:
        axR = axes[1]
        vx, vy = shown[0], shown[1]
        dx, dy = _by_short(vx), _by_short(vy)
        cx, cy = consts[dx["name"]], consts[dy["name"]]
        (xlo, xhi), (ylo, yhi) = bounds[vx], bounds[vy]
        intx = dx["kind"] == "int"
        inty = dy["kind"] == "int"
        nx = int(min(40, xhi - xlo + 1)) if intx else 40
        ny = int(min(40, yhi - ylo + 1)) if inty else 40
        xs = (np.arange(int(xlo), int(xhi) + 1) if intx and (xhi - xlo) <= 40
              else np.linspace(xlo, xhi, nx))
        ysr = (np.arange(int(ylo), int(yhi) + 1) if inty and (yhi - ylo) <= 40
               else np.linspace(ylo, yhi, ny))
        feas_x, feas_y, wit_x, wit_y = [], [], [], []
        base = z3.Solver(); base.add(body)
        for xv in xs:
            for yv in ysr:
                base.push()
                base.add(cx == (int(round(xv)) if intx else float(xv)))
                base.add(cy == (int(round(yv)) if inty else float(yv)))
                ok = base.check() == z3.sat
                base.pop()
                if ok:
                    feas_x.append(xv); feas_y.append(yv)
        if feas_x:
            axR.scatter(feas_x, feas_y, s=26, marker="s", color="#3fb950", alpha=0.35,
                        edgecolors="none", label="feasible (solved per cell)")
        axR.set_xlabel(vx); axR.set_ylabel(vy)
        axR.set_title(f"feasible region ({vx}, {vy}) — solved per cell, not a box", fontsize=11)
        axR.grid(alpha=0.2)
        if feas_x:
            axR.legend(loc="best", fontsize=9)

    fig.suptitle(f"{name} — solution space (a claim) · boundaries z3-solved exact",
                 fontsize=13)
    fig.tight_layout(rect=[0, 0, 1, 0.96])
    fig.savefig(out_path, dpi=120); plt.close(fig)
    return out_path


def main(argv):
    if len(argv) < 4:
        print("usage: render_claim_space.py <smt2> <schema> <out.png>")
        return 2
    render(argv[1], argv[2], argv[3])
    return 0


if __name__ == "__main__":
    import sys
    raise SystemExit(main(sys.argv))
