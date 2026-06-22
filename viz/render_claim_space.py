"""claim_space — the solution space of a raw CLAIM (a static constraint, not an FSM).

A claim has NO runs — so this view is entirely SOLVED, never sampled:
  * left  — each numeric variable's EXACT range over the solution set, via z3 Optimize
            (a provable sup/inf of the variable subject to the whole constraint).
  * right — the real FEASIBLE REGION of the two principal variables: we grid value-space and
            ask the solver, per cell, "is (x, y) extensible to a full satisfying assignment?"
            — shading the cells that are, so the boundary is the true edge of the solution set
            (not a bounding box). A few witness points are overlaid.

Consumes the `{"claim", "vars":[{name,kind,role}]}` schema that `evident export` emits for claims.
Seq/enum vars aren't listed (they live in the smt2 as arrays/datatypes); we render over the numeric
vars and tolerate an empty list.
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
    """Exact sup/inf of constant c over the constraint, or None if unbounded/unsat."""
    o = z3.Optimize()
    o.add(body)
    o.maximize(c) if maximize else o.minimize(c)
    if o.check() != z3.sat:
        return None
    val = _num(o.model().eval(c, model_completion=True))
    return val


def render(smt2_path, schema_path, out_path):
    sch, body, consts = _load_claim(smt2_path, schema_path)
    name = sch.get("claim", "claim")
    numeric = [v for v in sch.get("vars", [])
               if v.get("kind") in ("int", "real") and v["name"] in consts]
    if not numeric:
        return _na(out_path, f"{name} — solution space",
                   "this claim has no numeric variable to bound\n"
                   "(its variables are Seq/enum — press ⊨ Solve for a witness)")

    # exact bounds (z3 Optimize) for every numeric var
    bounds = {}
    for v in numeric:
        c = consts[v["name"]]
        lo = _opt_bound(body, c, maximize=False)
        hi = _opt_bound(body, c, maximize=True)
        bounds[_SHORT(v["name"])] = (lo, hi)
    shown = [n for n in (_SHORT(v["name"]) for v in numeric)
             if None not in bounds[n]]
    if not shown:
        return _na(out_path, f"{name} — solution space",
                   "the claim's numeric variables are unbounded\n(no finite solution-space boundary)")

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

    # --- right: the real feasible region — per-cell solve, not a bounding box ---
    if have2d:
        axR = axes[1]
        vx, vy = shown[0], shown[1]
        cx, cy = consts[_full(numeric, vx)], consts[_full(numeric, vy)]
        (xlo, xhi), (ylo, yhi) = bounds[vx], bounds[vy]
        intx = _kind(numeric, vx) == "int"
        inty = _kind(numeric, vy) == "int"
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


def _full(numeric, short):
    for v in numeric:
        if _SHORT(v["name"]) == short:
            return v["name"]
    return short


def _kind(numeric, short):
    for v in numeric:
        if _SHORT(v["name"]) == short:
            return v["kind"]
    return "int"


def main(argv):
    if len(argv) < 4:
        print("usage: render_claim_space.py <smt2> <schema> <out.png>")
        return 2
    render(argv[1], argv[2], argv[3])
    return 0


if __name__ == "__main__":
    import sys
    raise SystemExit(main(sys.argv))
