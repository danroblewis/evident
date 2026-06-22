"""solution_space — the SOLVED boundary of a program's variables, NOT a single run.

The trajectory views (time_series, phase_portrait) draw ONE orbit through state space.
This view draws the BOUNDARY of what is possible:
  * left  — each numeric variable's exact range over the whole reachable set, as a bar
            ("the abstract boundary of the variable"). Exact when the reachable set is
            finite and fully explored (an exhaustive solve); a lower bound when capped.
  * right — the feasible REGION of the two principal variables as a SET of points inside
            their bounding box, with fixed points / equilibria marked. The set, not a path.
"""
import json
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.patches import Rectangle
from evident_viz import load

_SHORT = lambda n: n.split(".")[-1]


def _write_points(out_path, points):
    """Sidecar for the interactive hover overlay: each plotted scatter point's FRACTIONAL
    position within the figure (fx, fy from the TOP-LEFT, both 0..1) plus its full state dict.
    Empty list for the 1-var / no-scatter / N-A cases — the overlay then no-ops."""
    try:
        with open(out_path + ".points.json", "w") as f:
            json.dump(points, f)
    except Exception:
        pass


def _full(m, short_name):
    for v in m.carried:
        if _SHORT(v["name"]) == short_name:
            return v["name"]
    return short_name


def _na(out_path, title, msg):
    fig, ax = plt.subplots(figsize=(9, 6))
    ax.text(0.5, 0.5, msg, ha="center", va="center", transform=ax.transAxes, fontsize=13)
    ax.set_xticks([]); ax.set_yticks([])
    ax.set_title(title, fontsize=13)
    fig.tight_layout(); fig.savefig(out_path, dpi=120); plt.close(fig)
    _write_points(out_path, [])
    return out_path


def render(smt2_path, schema_path, out_path):
    m = load(smt2_path, schema_path)
    states, edges = m.reachable(limit=400)
    struct = m.solution_structure(states=states, edges=edges)
    fps = struct.get("fixed_points", [])
    verdict = struct.get("verdict", "")
    n = struct.get("reachable", len(states))

    # PROVEN bounds via z3 Optimize over a k-step unrolling of the transition (#134) — NOT the BFS
    # sample. `exact` means the k-step and 2k-step bounds agree, so the reachable extent has closed
    # (the user's "compose with itself until 2-run == k-run" fixpoint). Fall back to the BFS bounds
    # (exact only when the reachable set was finite & fully explored) if Optimize is unavailable.
    # solved_bounds (z3 Optimize over the unroll) is cheap for DETERMINISTIC systems (counter ~10ms,
    # oscillator ~20ms) and gives the inductive/horizon proof. For NONDETERMINISTIC ones (vending,
    # pick — a free input each tick) the Optimize searches every input sequence over the unroll —
    # ~2s — yet the BFS bounds are ALREADY exact there when the reachable set is finite. So gate the
    # expensive prover on determinism; nondeterministic systems take the fast exact-by-exhaustion BFS
    # bounds. (Latency #188: vending 2.5s → ~0.1s, no loss of correctness.)
    solved = None
    if struct.get("branching", 1) <= 1:
        try:
            solved = m.solved_bounds(k=12)
        except Exception:
            solved = None
    if solved:
        bounds = {nm: [d["lo"], d["hi"]] for nm, d in solved.items()
                  if d["lo"] is not None and d["hi"] is not None}
        kk = next(iter(solved.values()))["k"]
        capped = struct.get("capped", False)
        all_inductive = bool(bounds) and all(solved[nm]["inductive"] for nm in bounds)
        all_tight = bool(bounds) and all(solved[nm]["tight"] for nm in bounds)
        # Honest epistemic ladder (Ana #138): an inductive invariant is a PROOF; a finite reachable
        # set fully explored is exact by exhaustion; k-vs-2k agreement alone is strong evidence over
        # a horizon, not a proof; otherwise it's only proven for the unrolled horizon.
        if all_inductive:
            boundtag = "exact — z3-proven INDUCTIVE invariant (the box is closed under the transition)"
        elif not capped:
            boundtag = "exact — finite reachable set, exhaustively explored"
        elif all_tight:
            boundtag = f"tight — k and 2k-step z3 Optimize agree (a {kk}-step horizon, not an inductive proof)"
        else:
            boundtag = f"z3-proven over a {kk}-step unrolling (Optimize) — may extend further"
        all_exact = all_inductive or not capped
        proven = True
    else:
        bounds = struct.get("bounds", {})
        capped = struct.get("capped", False)
        all_exact, proven = (not capped), False
        boundtag = (f"sampled over {n} reachable states — not exhaustive (true range may differ)"
                    if capped else f"exact — all {n} reachable states (exhaustively explored)")

    numeric = [_SHORT(v["name"]) for v in m.carried if v.get("kind") in ("int", "real")]
    numeric = [nm for nm in numeric if nm in bounds]
    if not numeric:
        return _na(out_path, f"{m.fsm} — solution space",
                   "solution space needs a numeric variable\n(this program's state is categorical —\nsee state_graph for its boundary)")
    enums = [_SHORT(v["name"]) for v in m.carried if v.get("kind") == "enum"]
    # The right panel needs a SECOND axis. Prefer a 2nd numeric var; failing that, put an ENUM on a
    # categorical y-axis so an enum+numeric machine (traffic: light × timer) still shows WHICH
    # (state, value) combinations are reachable — "is timer ever 2 while light=Green?" — instead of
    # dropping the enum and showing the numeric bar alone (Ana #115).
    pair_mode = "numeric" if len(numeric) >= 2 else ("enum" if enums else None)
    have2d = pair_mode is not None
    fig, axes = plt.subplots(1, 2 if have2d else 1,
                             figsize=(14 if have2d else 8.5, 6.5))
    axL = axes[0] if have2d else axes

    # --- left: each variable's solved boundary as a horizontal range bar ---
    ys = list(range(len(numeric)))
    for y, nm in zip(ys, numeric):
        lo, hi = bounds[nm]
        axL.plot([lo, hi], [y, y], lw=9, solid_capstyle="round", color="#58a6ff", alpha=0.5)
        axL.plot([lo, hi], [y, y], "|", color="#0f1419", markersize=16, markeredgewidth=2)
        axL.text(lo, y + 0.2, f"{lo:g}", ha="left", va="bottom", fontsize=9, color="#7d8590")
        axL.text(hi, y + 0.2, f"{hi:g}", ha="right", va="bottom", fontsize=9, color="#7d8590")
    axL.set_yticks(ys); axL.set_yticklabels(numeric)
    axL.set_ylim(-0.7, len(numeric) - 0.3)
    axL.set_xlabel("value spanned over the whole solution space")
    axL.set_title(f"variable boundaries — {boundtag}", fontsize=11)
    axL.grid(axis="x", alpha=0.2)

    # --- right: feasible region of the top-2 vars as a SET (no trajectory) + boundary box ---
    plotted = []   # (data_x, data_y, full_state_dict) for each scatter point — for the hover sidecar
    axR = None
    if pair_mode == "enum":
        # categorical y (enum variants) × numeric x — the reachable (state, value) pairs.
        axR = axes[1]
        vx, en = numeric[0], enums[0]
        fxn, fen = _full(m, vx), _full(m, en)
        variants = m.enum_variants.get(fen) or sorted({s.get(fen) for s in states if s.get(fen) is not None})
        vidx = {nm: i for i, nm in enumerate(variants)}
        for s in states:
            x, e = s.get(fxn), s.get(fen)
            if isinstance(x, (int, float)) and e in vidx:
                plotted.append((x, vidx[e], {_SHORT(k): v for k, v in s.items()}))
        if plotted:
            axR.scatter([p[0] for p in plotted], [p[1] for p in plotted], s=44, color="#58a6ff",
                        alpha=0.55, edgecolors="none", label="reachable")
        for f in fps:                                      # equilibria, if any land on this pair
            if vx in f and f.get(en) in vidx:
                axR.scatter([f[vx]], [vidx[f[en]]], marker="*", s=280, color="#c9a8ff",
                            edgecolors="#0f1419", zorder=5, label="fixed point")
        handles, labels = axR.get_legend_handles_labels()
        uniq = dict(zip(labels, handles))
        if uniq:
            axR.legend(uniq.values(), uniq.keys(), loc="best", fontsize=9)
        axR.set_yticks(range(len(variants))); axR.set_yticklabels(variants)
        axR.set_ylim(-0.6, len(variants) - 0.4)
        axR.set_xlabel(vx); axR.set_ylabel(en)
        axR.set_title(f"reachable ({en}, {vx}) pairs — every state×value combination the machine enters",
                      fontsize=11)
        axR.grid(axis="x", alpha=0.2)
    elif have2d:
        axR = axes[1]
        vx, vy = numeric[0], numeric[1]
        fxn, fyn = _full(m, vx), _full(m, vy)
        for s in states:
            x, y = s.get(fxn), s.get(fyn)
            if isinstance(x, (int, float)) and isinstance(y, (int, float)):
                plotted.append((x, y, {_SHORT(k): v for k, v in s.items()}))
        if plotted:
            px = [p[0] for p in plotted]; py = [p[1] for p in plotted]
            axR.scatter(px, py, s=24, color="#58a6ff", alpha=0.5, edgecolors="none",
                        label="reachable set")
        (xlo, xhi), (ylo, yhi) = bounds[vx], bounds[vy]
        axR.add_patch(Rectangle((xlo, ylo), (xhi - xlo) or 1, (yhi - ylo) or 1, fill=False,
                                edgecolor="#7ee0c0", lw=1.6, ls="--", label="bounding box"))
        for f in fps:
            if vx in f and vy in f:
                axR.scatter([f[vx]], [f[vy]], marker="*", s=280, color="#c9a8ff",
                            edgecolors="#0f1419", zorder=5, label="fixed point")
        handles, labels = axR.get_legend_handles_labels()
        uniq = dict(zip(labels, handles))
        if uniq:
            axR.legend(uniq.values(), uniq.keys(), loc="best", fontsize=9)
        axR.set_xlabel(vx); axR.set_ylabel(vy)
        axR.set_title(f"reachable set ({vx}, {vy}) — every reachable combination + its extent",
                      fontsize=11)
        axR.grid(alpha=0.2)

    framing = ("boundary z3-proven exact" if (proven and all_exact)
               else "boundary z3-proven over horizon" if proven
               else "boundary exhaustively solved" if all_exact
               else "boundary sampled (capped)")
    fig.suptitle(f"{m.fsm} — solution space · {verdict} · {framing}", fontsize=13)
    fig.tight_layout(rect=[0, 0, 1, 0.96])
    # Layout is final after tight_layout: map each scatter point's DATA coords → figure fraction.
    # transData → display px → transFigure⁻¹ → fraction with bottom-left origin; flip y to top-left.
    points = []
    if axR is not None and plotted:
        inv = fig.transFigure.inverted()
        for dx, dy, st in plotted:
            disp = axR.transData.transform((dx, dy))
            ffx, ffy = inv.transform(disp)
            if 0.0 <= ffx <= 1.0 and 0.0 <= ffy <= 1.0:
                points.append({"fx": round(float(ffx), 4),
                               "fy": round(float(1.0 - ffy), 4), "state": st})
    fig.savefig(out_path, dpi=120); plt.close(fig)
    _write_points(out_path, points)
    return out_path


def main(argv):
    if len(argv) < 4:
        print("usage: render_solution_space.py <smt2> <schema> <out.png>")
        return 2
    render(argv[1], argv[2], argv[3])
    return 0


if __name__ == "__main__":
    import sys
    raise SystemExit(main(sys.argv))
