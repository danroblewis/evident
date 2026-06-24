"""reachable_region.py — ABSTRACT reachable-region analysis for an FSM.

The dynamics counterpart to terminal_states.py. Instead of enumerating the reachable set, we
PROVE a bounding box for it by k-induction (k=1) over the one-step relation — sound, cheap, and
decides BOUNDED vs UNBOUNDED even when the state space is infinite (a free random walk), where
full_state_graph() can't enumerate.

  box candidate  : proven_range(var) per numeric carried var (its proven one-step [lo,hi]); if any
                   is None the var is genuinely UNBOUNDED.
  1-induction    : base `first_tick=true ∧ ¬box` UNSAT (every init in the box) AND step
                   `first_tick=false ∧ box(_x) ∧ ¬box(x)` UNSAT (box closed under the transition)
                   ⇒ the reachable set ⊆ box, PROVEN (a sound over-approximation; k>1 tightens).

  bounding_box(m) -> {"verdict": "bounded"|"unbounded"|"unknown", "box": {name:(lo,hi)},
                      "unbounded": [names], "inductive": bool, "note"}
"""
import z3

_SCALAR_NUM = ("int", "real")


def _I(m, numeric, box, key):
    """The box predicate over the current (key='name') or prev (key='prev') consts."""
    terms = []
    for v in numeric:
        c = m.consts[v[key]]
        lo, hi = box[v["name"]]
        terms += [c >= lo, c <= hi]
    return z3.And(*terms)


def bounding_box(m):
    numeric = [v for v in m.carried if v["kind"] in _SCALAR_NUM]
    if not numeric:
        return {"verdict": "unknown", "box": {}, "unbounded": [], "inductive": False,
                "note": "no numeric carried state to bound"}
    box, unbounded = {}, []
    for v in numeric:
        r = m.proven_range(v)               # proven_range takes the var DICT
        if r is None:
            unbounded.append(v["name"])
        else:
            box[v["name"]] = r
    if unbounded:
        return {"verdict": "unbounded", "box": box, "unbounded": unbounded,
                "inductive": False, "note": None}

    reln = z3.And(*list(m.assertions))
    base = z3.Solver(); base.set("timeout", 5000)
    base.add(reln)
    if m.first_tick is not None:
        base.add(m.first_tick == True)      # noqa: E712
    base.add(z3.Not(_I(m, numeric, box, "name")))
    step = z3.Solver(); step.set("timeout", 5000)
    step.add(reln)
    if m.first_tick is not None:
        step.add(m.first_tick == False)     # noqa: E712
    step.add(_I(m, numeric, box, "prev"))
    step.add(z3.Not(_I(m, numeric, box, "name")))
    inductive = base.check() == z3.unsat and step.check() == z3.unsat
    return {"verdict": "bounded", "box": box, "unbounded": [], "inductive": inductive,
            "note": None if inductive else
            "per-var one-step range; not proven closed under the transition (over-approximation only)"}
