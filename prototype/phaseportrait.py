"""phaseportrait — a GENERIC phase portrait generator for constraint models.

Give it a model = (named state variables + a transition relation written as a Z3
formula relating current vars `cur` to next vars `nxt`) and it draws the portrait
with NO per-model rendering code:

  • samples a grid over two chosen state axes,
  • asks Z3 for each grid point's successor(s) — so it handles FUNCTIONAL and
    RELATIONAL (nondeterministic) transitions, INTEGER or REAL state, alike,
  • draws the flow (a direction field for continuous, a fan for discrete),
    forward trajectories, and approximate fixed points,
  • and, given `init` + `bad`, runs Spacer to label a claimed trapping box
    PROVED / REFUTED.

The same `render()` produces every panel in the demo below — two models it was
built around (a damped oscillator, the queue daemon) and two it had never seen
(Lotka–Volterra, a double-well), proving it is model-agnostic.

What is NOT generic (inherent, see module docstring notes): models must be
Z3-expressible (no sin/exp — transcendental dynamics are out); >2 state variables
must be PROJECTED to a 2-D slice (holding the rest fixed); and the artisanal
polish of a bespoke panel (energy contours, curated annotations) is not
synthesized — the engine draws the correct skeleton, you decorate if you wish.

Run from prototype/:  python3 phaseportrait.py  -> results/phaseportrait_generic.png
"""
import os
from collections import deque

import numpy as np
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.patches import Rectangle

import z3

INK = "#2a2c34"; MUTED = "#6b7080"; GREEN = "#2eb55f"; GREENFILL = "#bfeccb"
RED = "#de3c3c"; FLOW = "#b9beca"; FAN = "#5a6478"; GHOST = "#e7e9ef"


# ─────────────────────────────── the model ──────────────────────────────────
class Model:
    """state: list of var names. transition(cur, nxt) -> z3 BoolRef, where cur/nxt
    are dicts {name: z3 var}. sorts: "Int"/"Real" (uniform) or a per-name dict.
    init: dict {name: value} or fn(cur)->BoolRef. bad: fn(cur)->BoolRef (unsafe)."""
    def __init__(self, name, state, transition, sorts="Real", init=None, bad=None):
        self.name = name
        self.state = list(state)
        self.transition = transition
        self.sorts = sorts if isinstance(sorts, dict) else {v: sorts for v in state}
        self.init = init
        self.bad = bad

    def var(self, name, suffix=""):
        mk = z3.Real if self.sorts[name] == "Real" else z3.Int
        return mk(name + suffix)

    def val(self, name, x):
        return z3.RealVal(float(x)) if self.sorts[name] == "Real" else z3.IntVal(int(x))


def _num(z3val):
    try:
        return z3val.as_long()
    except Exception:
        return float(z3val.as_fraction())


def successors(model, point, kmax=1):
    """All (up to kmax) next-states Z3 finds for a concrete current `point`."""
    cur = {v: model.var(v) for v in model.state}
    nxt = {v: model.var(v, "_n") for v in model.state}
    s = z3.Solver()
    for v in model.state:
        s.add(cur[v] == model.val(v, point[v]))
    s.add(model.transition(cur, nxt))
    out = []
    while len(out) < kmax and s.check() == z3.sat:
        m = s.model()
        succ = {v: _num(m.eval(nxt[v], model_completion=True)) for v in model.state}
        out.append(succ)
        s.add(z3.Or([nxt[v] != model.val(v, succ[v]) for v in model.state]))
    return out


def spacer_verdict(model):
    """Generic Spacer query: is `bad` reachable from `init` under `transition`?"""
    fp = z3.Fixedpoint(); fp.set(engine="spacer")
    sorts = [z3.RealSort() if model.sorts[v] == "Real" else z3.IntSort()
             for v in model.state]
    Inv = z3.Function("Inv", *sorts, z3.BoolSort())
    fp.register_relation(Inv)
    cur = {v: model.var(v) for v in model.state}
    nxt = {v: model.var(v, "_n") for v in model.state}
    for v in list(cur.values()) + list(nxt.values()):
        fp.declare_var(v)
    C = [cur[v] for v in model.state]; N = [nxt[v] for v in model.state]
    if callable(model.init):
        fp.rule(Inv(*C), [model.init(cur)])
    else:
        fp.rule(Inv(*[model.val(v, model.init[v]) for v in model.state]))
    fp.rule(Inv(*N), [Inv(*C), model.transition(cur, nxt)])
    return fp.query(z3.And(Inv(*C), model.bad(cur)))


# ─────────────────────────────── the renderer ───────────────────────────────
def _grid(model, name, lo, hi, n):
    if model.sorts[name] == "Int":
        return list(range(int(lo), int(hi) + 1))
    return list(np.linspace(lo, hi, n))


def _statekey(model, st):
    return tuple(round(float(st[v]), 6) for v in model.state)


def _reachable_set(model, inits, xaxis, yaxis, xr, yr, fixed, max_succ, cap=4000):
    """BFS the states reachable from `inits` (a list of full-state dicts), keeping
    only those that land within the plotted window (so unbounded models stay
    finite). Returns the set of state-keys."""
    seen, dq = {}, deque()
    for init in inits:
        st = {**fixed, **init}
        if all(v in st for v in model.state):
            k = _statekey(model, st)
            seen.setdefault(k, st)
            if k in seen:
                dq.append(st)
    while dq and len(seen) < cap:
        st = dq.popleft()
        for succ in successors(model, st, max_succ):
            k = _statekey(model, succ)
            if k in seen:
                continue
            if (xr[0] - 1.0 <= succ[xaxis] <= xr[1] + 1.0 and
                    yr[0] - 1.0 <= succ[yaxis] <= yr[1] + 1.0):
                seen[k] = succ; dq.append(succ)
    return set(seen.keys())


def _is_deterministic(model, XS, YS, xaxis, yaxis, fixed, probes=12):
    """Sample some grid states: if every one has a single successor, the model is
    deterministic and should be drawn as ORBITS, not a (misleading) field."""
    seen = 0
    for X in XS[::max(1, len(XS) // 4)]:
        for Y in YS[::max(1, len(YS) // 4)]:
            pt = {**fixed, xaxis: X, yaxis: Y}
            if len(successors(model, pt, 2)) > 1:
                return False
            seen += 1
            if seen >= probes:
                return True
    return True


def render(ax, model, xaxis, yaxis=None, xr=(0, 1), yr=None, *, fixed=None,
           n=21, max_succ=1, style="field", seeds=(), tsteps=360, safe_box=None,
           prove=False, title=None, traj_colors=None, equal=False, reachable=None,
           orbits=None, paths=None):
    oned = yaxis is None                              # 1-D state -> a number line
    fixed = fixed or {}
    if oned and yr is None:
        yr = (-0.8, 0.8)
    XS = _grid(model, xaxis, xr[0], xr[1], n)
    YS = [0.0] if oned else _grid(model, yaxis, yr[0], yr[1], n)

    def disp(s, X, Y):
        return s[xaxis] - X, (0.0 if oned else s[yaxis] - Y)

    # reachable set from `reachable` (a full-state dict or list of them): ghost the
    # rest, so the program (the reachable orbit/region) stands out from the relation.
    reach_keys = None
    if reachable is not None and not oned:
        inits = reachable if isinstance(reachable, (list, tuple)) else [reachable]
        reach_keys = _reachable_set(model, inits, xaxis, yaxis, xr, yr, fixed, max_succ)

    # claimed trapping box + Spacer verdict (2-D only)
    verdict = None
    if safe_box is not None and not oned:
        (bx0, bx1) = safe_box[xaxis]; (by0, by1) = safe_box[yaxis]
        ok = True
        if prove and model.init is not None and model.bad is not None:
            verdict = spacer_verdict(model)
            ok = verdict == z3.unsat
        col = GREEN if ok else RED
        ax.add_patch(Rectangle((bx0, by0), bx1 - bx0, by1 - by0,
                               facecolor=GREENFILL if ok else "#f6d4d4",
                               edgecolor=col, lw=2.3, zorder=0))

    # deterministic models get ORBITS (a field falsely implies disjoint orbits
    # merge — see phase-portraits.md "a direction field lies for injective maps").
    det = orbits
    if det is None and not oned:
        det = _is_deterministic(model, XS, YS, xaxis, yaxis, fixed)
    det = bool(det) and not oned

    # one grid pass → successors, fixed points, reachability; plus field arrows OR
    # the deterministic next-state map for tracing orbits.
    fx, fy, fu, fv, areach = [], [], [], [], []
    fixed_pts, lattice, reach_pts, nextof = [], [], [], {}
    for X in XS:
        for Y in YS:
            lattice.append((X, Y))
            pt = {**fixed, xaxis: X}
            if not oned:
                pt[yaxis] = Y
            src_r = reach_keys is None or _statekey(model, pt) in reach_keys
            if reach_keys is not None and src_r:
                reach_pts.append((X, Y))
            succs = successors(model, pt, 1 if det else max_succ)
            moved = [s for s in succs if max(map(abs, disp(s, X, Y))) > 1e-9]
            if succs and not moved:               # ONLY self-loops -> true fixed point
                fixed_pts.append((X, Y))
            if det:
                if moved:
                    nextof[(X, Y)] = (moved[0][xaxis], moved[0][yaxis])
            else:
                for s in moved:                   # an idle self-loop just isn't drawn
                    d0, d1 = disp(s, X, Y)
                    fx.append(X); fy.append(Y); fu.append(d0); fv.append(d1)
                    areach.append(src_r)

    if det:                                           # trace each orbit once (dedup)
        drawn = set()
        for start in nextof:
            if start in drawn:
                continue
            path, cur = [start], start
            for _ in range(24):
                drawn.add(cur)
                nx = nextof.get(cur)
                if nx is None:
                    break
                path.append(nx)
                if nx in drawn or nx not in nextof:
                    break
                cur = nx
            if len(path) > 1:
                px, py = zip(*path)
                ax.plot(px, py, "-", color=GHOST, lw=1.0, alpha=0.9, zorder=1)
    else:                                             # direction / fan field
        fu, fv = np.array(fu, float), np.array(fv, float)
        if style == "field" and len(fu):              # normalize to a direction field
            L = np.hypot(fu, fv); L[L == 0] = 1
            step = 0.62 * min((xr[1] - xr[0]) / n, (yr[1] - yr[0]) / n)
            fu, fv = fu / L * step, fv / L * step
        elif len(fu):                                 # discrete: shrink the real jump
            fu, fv = fu * 0.6, fv * 0.6
        base_col = FLOW if style == "field" else FAN
        qcolor = ([base_col if r else GHOST for r in areach]
                  if reach_keys is not None else base_col)
        ax.quiver(fx, fy, fu, fv, color=qcolor, angles="xy", scale_units="xy",
                  scale=1, width=0.004, headwidth=4, headlength=5, zorder=2)
        if style != "field" and lattice:              # show the discrete lattice
            lx, ly = zip(*lattice)
            ax.plot(lx, ly, "o", color=GHOST if reach_keys is not None else FAN,
                    ms=2.3, zorder=1)
    if reach_pts:                                     # highlight the reachable states
        rx, ry = zip(*reach_pts)
        ax.plot(rx, ry, "o", color=INK, ms=3.4, zorder=4)

    # forward trajectories (one run; for relational, first successor each step)
    cols = traj_colors or ["#2868d2", "#9646c8", "#00a0a0", "#eb9628", "#d23bb0"]
    for i, seed in enumerate(seeds):
        pt = dict(seed)
        xs, ys = [pt[xaxis]], [0.0 if oned else pt[yaxis]]
        for _ in range(tsteps):
            sc = successors(model, {**fixed, **pt}, 1)
            if not sc:
                break
            pt = sc[0]; xs.append(pt[xaxis]); ys.append(0.0 if oned else pt[yaxis])
        ax.plot(xs, ys, color=cols[i % len(cols)], lw=1.8,
                marker="o" if oned else None, ms=4, zorder=3)
    for i, p in enumerate(paths or []):               # explicit concrete trajectories
        if p:
            px, py = zip(*p)
            ax.plot(px, py, "-o", color=cols[i % len(cols)], lw=2.2, ms=4, zorder=4)

    for (X, Y) in fixed_pts:                          # fixed points / halt set
        pt = {**fixed, xaxis: X}
        if not oned:
            pt[yaxis] = Y
        fp_ghost = reach_keys is not None and _statekey(model, pt) not in reach_keys
        ax.plot(X, Y, "o", color=GHOST if fp_ghost else INK, ms=5,
                mfc="white", mew=1.4, zorder=5)

    ax.set_xlim(*xr); ax.set_ylim(*yr)
    ax.set_xlabel(xaxis, fontsize=9, color=MUTED)
    ax.tick_params(colors=MUTED, labelsize=7.5)
    if oned:
        ax.set_yticks([])
    else:
        ax.set_ylabel(yaxis, fontsize=9, color=MUTED, rotation=0, labelpad=10)
    for sp in ax.spines.values():
        sp.set_color("#d2d6de")
    ax.set_title(title if title is not None else model.name,
                 fontsize=11.5, color=INK, loc="left", pad=7)
    if verdict is not None:
        ok = verdict == z3.unsat
        ax.text(0.5, -0.17, "Spacer: UNSAT — proved safe" if ok
                else "Spacer: SAT — box refuted", transform=ax.transAxes,
                ha="center", fontsize=9.5, weight="bold", color=GREEN if ok else RED)
    if equal and not oned:
        ax.set_aspect("equal")


# ─────────────────────────────── demo models ────────────────────────────────
def damped_oscillator():
    dt = 0.08
    def T(c, n):
        return z3.And(n["x"] == c["x"] + dt * c["v"],
                      n["v"] == c["v"] + dt * (-c["x"] - 0.22 * c["v"]))
    return Model("Damped oscillator  (recovered)", ["x", "v"], T, "Real")


def lotka_volterra():
    dt, a, b, cc, d = 0.008, 1.1, 0.4, 0.1, 0.4       # fixed point at (d/cc, a/b)=(4, 2.75)
    def T(c, n):                                       # predator-prey (polynomial)
        return z3.And(n["x"] == c["x"] + dt * (a * c["x"] - b * c["x"] * c["y"]),
                      n["y"] == c["y"] + dt * (cc * c["x"] * c["y"] - d * c["y"]))
    return Model("Lotka–Volterra  (FRESH)", ["x", "y"], T, "Real")


def double_well():
    dt = 0.03
    def T(c, n):                                       # bistable: two basins + saddle
        return z3.And(n["x"] == c["x"] + dt * c["y"],
                      n["y"] == c["y"] + dt * (c["x"] - c["x"] * c["x"] * c["x"]
                                               - 0.3 * c["y"]))
    return Model("Double-well  (FRESH)", ["x", "y"], T, "Real")


def queue_daemon(CAP=6):
    def T(c, n):
        q0, q1, m0, m1 = c["q0"], c["q1"], n["q0"], n["q1"]
        return z3.Or(z3.And(q0 < CAP, m0 == q0 + 1, m1 == q1),
                     z3.And(q0 > 0, q1 < CAP, m0 == q0 - 1, m1 == q1 + 1),
                     z3.And(q1 > 0, m0 == q0, m1 == q1 - 1),
                     z3.And(m0 == q0, m1 == q1))
    bad = lambda c: z3.Or(c["q0"] < 0, c["q0"] > CAP, c["q1"] < 0, c["q1"] > CAP)
    return Model("Queue daemon  (recovered)", ["q0", "q1"], T, "Int",
                 init={"q0": 0, "q1": 0}, bad=bad)


def main():
    fig, axes = plt.subplots(2, 2, figsize=(13.5, 13))
    fig.suptitle("Generic phase portrait generator — one engine, any constraint model",
                 fontsize=17, color=INK, weight="bold", y=0.975)
    fig.text(0.5, 0.94, "every panel is the SAME render() applied to a different Z3 "
             "transition relation — two recovered, two it had never seen",
             ha="center", fontsize=10, color=MUTED)

    render(axes[0, 0], damped_oscillator(), "x", "v", (-3, 3), (-3, 3),
           style="field", seeds=[{"x": 2.6, "v": 0}, {"x": -2.6, "v": 0.4},
                                 {"x": 0.4, "v": 2.6}])
    render(axes[0, 1], lotka_volterra(), "x", "y", (0, 8), (0, 6),
           style="field", n=23,
           seeds=[{"x": 4, "y": 1.5}, {"x": 4, "y": 0.9}, {"x": 2.7, "y": 2.75}],
           tsteps=650)
    render(axes[1, 0], double_well(), "x", "y", (-2, 2), (-2, 2),
           style="field", n=23,
           seeds=[{"x": 1.8, "y": 1.2}, {"x": -1.8, "y": -1.2},
                  {"x": 0.05, "y": 1.6}, {"x": -0.05, "y": -1.6}])
    render(axes[1, 1], queue_daemon(), "q0", "q1", (-0.6, 7.6), (-0.6, 7.6),
           style="fan", max_succ=6, prove=True, equal=True,
           safe_box={"q0": (0, 6), "q1": (0, 6)})

    fig.tight_layout(rect=(0, 0, 1, 0.93))
    out = os.path.join(os.path.dirname(__file__), "results",
                       "phaseportrait_generic.png")
    os.makedirs(os.path.dirname(out), exist_ok=True)
    fig.savefig(out, dpi=130, facecolor="white")
    print("wrote", out, flush=True)


if __name__ == "__main__":
    main()
