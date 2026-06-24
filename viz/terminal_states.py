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


def _consts(expr):
    """Every uninterpreted 0-arg const in expr, deduped by id (iterative — these ASTs get deep)."""
    seen, out, stack = set(), [], [expr]
    while stack:
        e = stack.pop()
        if e.num_args() == 0 and e.decl().kind() == z3.Z3_OP_UNINTERPRETED:
            if e.get_id() not in seen:
                seen.add(e.get_id()); out.append(e)
        else:
            stack.extend(e.children())
    return out


def _escape_copy(reln, nexts, prevs, tag):
    """A fresh copy of the relation sharing ONLY the carried PREVs (the candidate state s); the carried
    next-states AND every free input are freshened. Sharing a free input (e.g. a nondeterministic
    `step`) would let the self-loop pin it, hiding the escaping successor — the unsoundness Ana #322
    found. Returns (fresh_vars, copied_reln, fresh_nexts)."""
    prev_ids = {p.get_id() for p in prevs}
    freevars = [c for c in _consts(reln) if c.get_id() not in prev_ids]
    have = {c.get_id() for c in freevars}
    for n in nexts:                       # a carried next absent from reln (unconstrained) still freshens
        if n.get_id() not in have:
            freevars.append(n); have.add(n.get_id())
    fresh = [z3.Const(str(c) + tag, c.sort()) for c in freevars]
    copied = z3.substitute(reln, *list(zip(freevars, fresh)))
    by_id = {c.get_id(): f for c, f in zip(freevars, fresh)}
    return fresh, copied, [by_id[n.get_id()] for n in nexts]


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
    # "Is there a DIFFERENT successor from the same prev?" — a fresh relation copy sharing only the
    # prevs (candidate s), with the nexts AND free inputs freshened so a nondeterministic input can
    # pick an escaping successor instead of being pinned by the self-loop (Ana #322).
    esc, esc_rel, esc_nexts = _escape_copy(reln, nexts, prevs, "__esc")
    escape = z3.And(esc_rel, z3.Or(*[esc_nexts[i] != prevs[i] for i in range(len(prevs))]))
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


def _is_deterministic(m):
    """Does the FSM have a UNIQUE successor per state (next a function of prev)? Two relation copies
    sharing prev (the 2nd with fresh nexts + inputs), asserting the nexts differ: UNSAT ⟹ deterministic.
    A free input that branches the successor makes the perturb-and-step direction ambiguous, so
    stability must not claim stable/unstable on it (Ana #323)."""
    carried = m.carried
    if not carried:
        return True
    nexts = [m.consts[v["name"]] for v in carried]
    prevs = [m.consts[v["prev"]] for v in carried]
    reln = z3.And(*list(m.assertions))
    if m.first_tick is not None:
        reln = z3.And(reln, m.first_tick == False)             # noqa: E712
    _, rel2, nexts2 = _escape_copy(reln, nexts, prevs, "__d2")
    s = z3.Solver(); s.set("timeout", 5000)
    s.add(reln); s.add(rel2)
    s.add(z3.Or(*[nexts[i] != nexts2[i] for i in range(len(nexts))]))
    return s.check() == z3.unsat


def _dist(a, b, numeric):
    return sum((a[v["name"]] - b[v["name"]]) ** 2 for v in numeric) ** 0.5


def stability(m, state, numeric):
    """Classify a fixed point by LOCAL FLOW: perturb each numeric var by ±1, take one step, and see
    whether the perturbed state moves TOWARD the fixed point (attracting), AWAY (repelling), or both
    (saddle). Discrete + deterministic-near-the-point; returns one of stable / unstable / saddle, or
    'unknown' when there are no numeric vars or no valid perturbation resolves. This is the bistable's
    0,6 (stable walls) vs 3 (unstable watershed) distinction the bare terminal set can't show."""
    if not numeric:
        return "unknown"
    if not _is_deterministic(m):           # a free input branches the successor — direction ambiguous (Ana #323)
        return "unknown"
    toward = away = 0
    for v in numeric:
        for d in (-1, 1):
            n = dict(state)
            n[v["name"]] = state[v["name"]] + d
            succ = m.successor(n)
            if succ is None:
                continue
            d_before, d_after = _dist(n, state, numeric), _dist(succ, state, numeric)
            if d_after < d_before - 1e-9:
                toward += 1
            elif d_after > d_before + 1e-9:
                away += 1
    if toward and away:
        return "saddle"
    if toward:
        return "stable"
    if away:
        return "unstable"
    return "unknown"
