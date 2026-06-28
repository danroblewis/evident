"""region_oracle.py — an ABSTRACT, exact reachable-set probe for a 2-D integer FSM.

The renderer's `.data.json` records the per-variable BOX (a hyper-rectangle). A golden test
that wants to assert the true reachable SHAPE — is it an L∞ square? an L1 diamond? — needs the
exact reachable set, not a box. This computes it the only sound way: by composing the FSM's
transition relation with itself k times (fresh per-tick consts, each `_prev` wired to the prior
tick — the same unrolling `model.solved_bounds` uses) and asking z3, for each lattice point,
"is (x=X, y=Y) attained at some tick 0..k from the seeded initial state?"

This is SLOW (one solver call per probed point) — it is a TEST oracle, run on tiny k over a small
window, NOT a renderer. Its whole job is to be the ground truth the renderer's box is judged against.

    reachable_set(model, "x", "y", k)  -> set[(X, Y)]  of integer points reachable within k ticks
"""
import z3


def _unrolled(model, k):
    """The k-step unrolling: a single z3 formula `base` over per-tick fresh consts, plus the
    per-tick step-var maps. Tick 0 is the seeded init (is_first_tick=true); ticks 1..k apply the
    ¬is_first_tick transition, each carried `_prev` wired to the prior tick's value."""
    body = z3.And(*model.assertions) if len(model.assertions) != 1 else model.assertions[0]
    ft = model.consts[model._first_tick_name]
    carried = [v for v in model.carried if v["kind"] in ("int", "real")]
    prev_to_cur = {model.consts[v["prev"]].get_id(): v["name"] for v in carried}
    non_ft = [(n, c) for n, c in model.consts.items() if n != model._first_tick_name]
    fresh = lambda c, tag: z3.Const(f"{c.decl().name()}@{tag}", c.sort())
    stepv = [{n: fresh(c, s) for n, c in non_ft if c.get_id() not in prev_to_cur}
             for s in range(k + 1)]
    initprev = {v["name"]: fresh(model.consts[v["prev"]], "init") for v in carried}
    clauses = []
    for s in range(k + 1):
        subs = [(ft, z3.BoolVal(s == 0))]
        for n, c in non_ft:
            if c.get_id() in prev_to_cur:
                cur = prev_to_cur[c.get_id()]
                subs.append((c, stepv[s - 1][cur] if s >= 1 else initprev[cur]))
            else:
                subs.append((c, stepv[s][n]))
        clauses.append(z3.substitute(body, *subs))
    return z3.And(*clauses), stepv


def reachable_set(model, xname, yname, k, window=None):
    """The EXACT set of integer (X, Y) points reachable within k ticks from the seeded init.
    `window` bounds the lattice probed (default k+1 past the box on each side, enough to also
    confirm NOTHING outside the true region is reachable). Returns a set of (int, int)."""
    base, stepv = _unrolled(model, k)
    w = window if window is not None else k + 1
    out = set()
    for X in range(-w, w + 1):
        for Y in range(-w, w + 1):
            s = z3.Solver()
            s.add(base)
            s.add(z3.Or(*[z3.And(stepv[t][xname] == X, stepv[t][yname] == Y)
                          for t in range(k + 1)]))
            if s.check() == z3.sat:
                out.add((X, Y))
    return out


def reachable_at_exactly(model, xname, yname, X, Y, k):
    """Is (X, Y) attained at EXACTLY tick k (not earlier)? Used to confirm the FRONTIER — e.g. a
    corner (k, k) that needs all k steps proves the region grows to its full extent each tick."""
    base, stepv = _unrolled(model, k)
    s = z3.Solver()
    s.add(base)
    s.add(stepv[k][xname] == X, stepv[k][yname] == Y)
    return s.check() == z3.sat
