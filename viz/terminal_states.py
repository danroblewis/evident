"""terminal_states.py — ABSTRACT terminal/absorbing-state analysis for an FSM.

A state s is ABSORBING (terminal) iff the one-step transition relation allows s -> s
AND admits NO successor other than s — once there, the FSM can only stay. We compute
the absorbing set as a quantified Z3 query over the transition relation
(`model.assertions` with is_first_tick = false), NOT by walking trajectories or
enumerating the bounded state product. That is the whole point: this decides the
terminal set even for UNBOUNDED models (a free random walk) where
`full_state_graph()` returns discrete=False and can't enumerate.

The verdict is the daemon-vs-terminates distinction, abstractly:
  * empty absorbing set  -> DAEMON: the FSM never comes to rest (its value is its
    ongoing/recurrent behavior — non-terminal states matter).
  * non-empty            -> it CAN terminate; the set is where it can come to rest.
  * Z3 unknown           -> the quantified query didn't decide (reported honestly).

NOTE: a non-empty absorbing set says the FSM CAN halt, not that EVERY run does — full
termination (every trajectory reaches the set) layers the BMC-completeness / fairness
machinery on top. This module answers "where can it end", abstractly.

  absorbing_states(m, limit) -> (states, decided)
  classify(m)                -> {"verdict", "states", "decided"}
"""
import z3


def absorbing_states(m, limit=64):
    """The terminal set {s : relation allows s->s AND no successor != s}, via Z3.
    Returns (list-of-state-dicts, decided); decided=False iff Z3 returned unknown."""
    carried = m.carried
    if not carried:
        return [], True
    nexts = [m.consts[v["name"]] for v in carried]
    prevs = [m.consts[v["prev"]] for v in carried]
    reln = z3.And(*list(m.assertions))
    if m.first_tick is not None:
        reln = z3.And(reln, m.first_tick == False)             # noqa: E712  steady-state dynamics
    # A renamed copy of the relation whose next-state is `esc`, used to ask "is there a
    # DIFFERENT successor from the same prev?". The prev consts stay shared (the candidate s).
    esc = [z3.Const(v["name"] + "__esc", nexts[i].sort()) for i, v in enumerate(carried)]
    esc_rel = z3.substitute(reln, *[(nexts[i], esc[i]) for i in range(len(nexts))])
    escape = z3.And(esc_rel, z3.Or(*[esc[i] != prevs[i] for i in range(len(prevs))]))
    s = z3.Solver()
    s.set("timeout", 5000)
    s.add(reln)
    for n, p in zip(nexts, prevs):
        s.add(n == p)                                          # a self-loop s->s is admissible
    s.add(z3.Not(z3.Exists(esc, escape)))                      # ...and NO other successor exists
    out, decided = [], True
    while len(out) < limit:
        r = s.check()
        if r == z3.unsat:
            break
        if r == z3.unknown:
            decided = False
            break
        mod = s.model()
        out.append(m._read_state(mod))
        s.add(m._block_clause(mod))
    return out, decided


_SCALAR = {"int", "real", "bool", "enum"}


def classify(m):
    """{'verdict': 'terminates'|'daemon'|'unknown', 'states': [...], 'decided': bool, 'note'}."""
    if any(v["kind"] not in _SCALAR for v in m.carried):
        return {"verdict": "unknown", "states": [], "decided": False,
                "note": "non-scalar carried state (Seq/record) — abstract terminal analysis "
                        "not supported yet"}
    states, decided = absorbing_states(m)
    verdict = "unknown" if not decided else ("terminates" if states else "daemon")
    return {"verdict": verdict, "states": states, "decided": decided, "note": None}
