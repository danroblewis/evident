"""successors.py — an independent ONE-STEP successor probe for the golden suite.

The state_graph / transition_matrix golden tests assert that the drawn graph's branching factor
matches what the model's transition relation CAN actually produce. To judge that without trusting
the renderer, we ask the transition directly: from a given state, how many distinct successor
states exist after exactly one ¬is_first_tick step?

For the random walk (Δx, Δy ∈ {-1,0,+1} independent) this is 9 — the king-move stencil. A renderer
that draws out-degree 1 or 4 has collapsed the nondeterminism, and the test compares the drawn
branching against THIS ground truth.

    one_step_successors(model, {"x": 0, "y": 0})  ->  set of frozenset(state.items())
"""
import z3


def one_step_successors(model, state, window=4):
    """Every distinct successor of `state` after exactly one ¬is_first_tick transition, enumerated
    by blocking each found assignment and re-solving. `state` keys are SHORT or full carried names;
    `window` bounds the integer search (the random walk moves ≤1 per axis, so ±window is ample).

    Returns a set of tuples sorted-by-name — `{( (xname, xval), (yname, yval) ), ...}`."""
    body = z3.And(*model.assertions) if len(model.assertions) != 1 else model.assertions[0]
    ft = model.consts.get(model._first_tick_name)
    numeric = [v for v in model.carried if v["kind"] in ("int", "real")]

    s = z3.Solver()
    s.add(body)
    if ft is not None:
        s.add(ft == False)                                     # noqa: E712 — a real transition step
    # pin the PREVIOUS-tick consts to `state` (accept short or full keys)
    short = {v["name"].split(".")[-1]: v for v in numeric}
    for k, val in state.items():
        v = short.get(k.split(".")[-1])
        if v is not None:
            s.add(model.consts[v["prev"]] == val)
    cur = {v["name"]: model.consts[v["name"]] for v in numeric}

    found = set()
    while len(found) < (2 * window + 1) ** max(1, len(numeric)) and s.check() == z3.sat:
        mdl = s.model()
        assign = tuple(sorted((v["name"].split(".")[-1], mdl.eval(c, model_completion=True).as_long())
                              for v, c in zip(numeric, cur.values())))
        if assign in found:
            break
        found.add(assign)
        s.add(z3.Or(*[c != mdl.eval(c, model_completion=True) for c in cur.values()]))
    return found
