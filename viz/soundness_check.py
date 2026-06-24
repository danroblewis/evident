"""On-demand soundness cross-check for the abstract views (#332, the user-facing half of #330).

Answers, for the model the user is looking at RIGHT NOW: does the abstract Z3 verdict match a
brute-force enumeration? Two independent checks on a complete bounded-discrete reachable graph:

  * ABSORBING — the abstract Z3 absorbing-set query (terminal_states.absorbing_states) must equal the
    brute-force "states whose only successor is themselves" over the reachable graph.
  * BOX — a k-induction reachable box claimed BOUNDED (reachable_region.bounding_box) must CONTAIN
    every brute-reachable state (it's an over-approximation).

This is the #330 fabrication-probe logic, applied live to the user's model instead of a CI fixture —
so an expert can push the button on their OWN program (Ana #332). Not applicable to real-valued /
capped models (not exactly enumerable); says so honestly rather than guessing.

  soundness_report(m) -> {"applicable", "absorbing_ok", "box_ok", "detail"}
"""
from terminal_states import absorbing_states, _key
from reachable_region import bounding_box


def soundness_report(m):
    if any(v.get("kind") == "real" for v in m.carried):
        return {"applicable": False, "verdict": "n/a", "detail": "real-valued — not exactly enumerable"}
    states, edges = m.reachable(limit=500)
    if len(states) >= 500 or not states:
        return {"applicable": False, "verdict": "n/a",
                "detail": "reachable set exceeds the 500-state enumeration cap"}

    # ABSORBING cross-check: abstract Z3 query vs the brute reachable graph (out-targets == {i}).
    absorbing_ok, detail = None, ""
    abs_states, decided = absorbing_states(m)
    if decided:
        out = {}
        for (i, j) in edges:
            out.setdefault(i, set()).add(j)
        brute = {_key(states[i]) for i, js in out.items() if js == {i}}
        reach = {_key(s) for s in states}
        abstract = {_key(s) for s in abs_states} & reach
        absorbing_ok = (abstract == brute)
        if not absorbing_ok:
            detail = f"absorbing: abstract {sorted(abstract)} != brute {sorted(brute)}"

    # BOX cross-check: a proven-bounded box must contain every reachable state.
    box_ok = None
    r = bounding_box(m)
    if r["verdict"] == "bounded":
        bad = [(v, s.get(v), rng) for s in states for v, rng in r["box"].items()
               if s.get(v) is not None and not (rng[0] <= s.get(v) <= rng[1])]
        box_ok = (len(bad) == 0)
        if bad:
            detail = (detail + f"; box: reachable {bad[0][0]}={bad[0][1]} outside {bad[0][2]}").strip("; ")

    # A verdict that NEVER claims a match it didn't compute (Ana #335): "sound" requires at least one
    # check to have actually run AND passed; both-None (Z3 undecided on absorbing + box not bounded) is
    # "inconclusive", not "✓". The exact fabrication this verifier exists to catch — turned on itself.
    if absorbing_ok is False or box_ok is False:
        verdict = "mismatch"
    elif absorbing_ok is True or box_ok is True:
        verdict = "sound"
    else:
        verdict = "inconclusive"
    return {"applicable": True, "verdict": verdict, "absorbing_ok": absorbing_ok,
            "box_ok": box_ok, "detail": detail}
