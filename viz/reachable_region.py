"""reachable_region.py — ABSTRACT reachable-region analysis for an FSM.

The dynamics counterpart to terminal_states.py. Instead of enumerating the reachable set we PROVE a
bounding box for it by k-induction (k=1) over the one-step relation — sound, cheap, and deciding
BOUNDED vs UNBOUNDED even on infinite state spaces.

  box candidate  : proven_range(var) per numeric carried var (its proven one-step [lo,hi]).
  1-induction    : base `first_tick=true ∧ ¬box` UNSAT (every init in the box) AND step
                   `first_tick=false ∧ box(_x) ∧ ¬box(x)` UNSAT (box closed under the transition)
                   ⇒ the reachable set ⊆ box, PROVEN (a sound over-approximation; k>1 tightens).

When a var's one-step IMAGE is unbounded (proven_range is None) we must NOT just declare the reachable
set unbounded — the image being unbounded doesn't mean the reachable set is (e.g. x=_x/2: the image is
all reals, but it contracts to a bounded set). So we split:
  * UNBOUNDED     — SOUNDLY proven: the var can grow without bound (a step increases/decreases it even
                    at arbitrarily large magnitude — `_provably_unbounded`). A free random walk.
  * INDETERMINATE — one-step image unbounded but no growth proof; the reachable set may still be
                    bounded by the dynamics. Honest "couldn't prove a finite bound".

  bounding_box(m) -> {"verdict": "bounded"|"unbounded"|"indeterminate"|"unknown",
                      "box": {name:(lo,hi)}, "unbounded": [names], "indeterminate": [names],
                      "inductive": bool, "note"}
"""
import z3

from model_global import _finite_numeric

_SCALAR_NUM = ("int", "real")


def _I(m, numeric, box, key):
    """The box predicate over the current (key='name') or prev (key='prev') consts."""
    terms = []
    for v in numeric:
        c = m.consts[v[key]]
        lo, hi = box[v["name"]]
        terms += [c >= lo, c <= hi]
    return z3.And(*terms)


def _provably_unbounded(m, v):
    """SOUND: v can grow without bound — a step increases (max) or decreases (min) v even at
    arbitrarily large magnitude. Distinguishes a genuine unbounded walk (Δx=1 always lets x grow)
    from a var whose one-step IMAGE is unbounded but whose REACHABLE set is bounded by a contraction
    (x=_x/2). ∞ is read off the Optimize objective handle via _finite_numeric (None == ±∞)."""
    nxt, prv = m.consts[v["name"]], m.consts[v["prev"]]
    # A CONCRETE step (≥ +1 / ≤ −1), not strict `>`/`<`: a strict bound makes the Optimum an open
    # supremum (z3 ε-representation) that _finite_numeric can't read as finite. Sound sufficient
    # condition (may under-claim a sub-unit-step real growth → that stays INDETERMINATE, honestly).
    for grow, sense in ((nxt >= prv + 1, "max"), (nxt <= prv - 1, "min")):
        opt = z3.Optimize(); opt.set("timeout", 4000)
        opt.add(m.assertions)
        if m.first_tick is not None:
            opt.add(m.first_tick == False)              # noqa: E712
        opt.add(grow)
        h = opt.maximize(prv) if sense == "max" else opt.minimize(prv)
        if opt.check() == z3.sat and \
                _finite_numeric(h.upper() if sense == "max" else h.lower()) is None:
            return True
    return False


def bounding_box(m):
    numeric = [v for v in m.carried if v["kind"] in _SCALAR_NUM]
    if not numeric:
        return {"verdict": "unknown", "box": {}, "unbounded": [], "indeterminate": [],
                "inductive": False, "note": "no numeric carried state to bound"}
    box, unbounded, indeterminate = {}, [], []
    for v in numeric:
        r = m.proven_range(v)                           # proven_range takes the var DICT
        if r is None:
            (unbounded if _provably_unbounded(m, v) else indeterminate).append(v["name"])
        else:
            box[v["name"]] = r
    if unbounded:
        return {"verdict": "unbounded", "box": box, "unbounded": unbounded,
                "indeterminate": indeterminate, "inductive": False, "note": None}
    if indeterminate:
        return {"verdict": "indeterminate", "box": box, "unbounded": [],
                "indeterminate": indeterminate, "inductive": False,
                "note": "one-step image unbounded but no growth proof — the reachable set may be "
                        "bounded by the dynamics (e.g. a contraction); a tighter invariant could decide"}

    reln = z3.And(*list(m.assertions))
    base = z3.Solver(); base.set("timeout", 5000)
    base.add(reln)
    if m.first_tick is not None:
        base.add(m.first_tick == True)                  # noqa: E712
    base.add(z3.Not(_I(m, numeric, box, "name")))
    step = z3.Solver(); step.set("timeout", 5000)
    step.add(reln)
    if m.first_tick is not None:
        step.add(m.first_tick == False)                 # noqa: E712
    step.add(_I(m, numeric, box, "prev"))
    step.add(z3.Not(_I(m, numeric, box, "name")))
    inductive = base.check() == z3.unsat and step.check() == z3.unsat
    return {"verdict": "bounded", "box": box, "unbounded": [], "indeterminate": [],
            "inductive": inductive, "note": None if inductive else
            "per-var one-step range; not proven closed under the transition (over-approximation only)"}
