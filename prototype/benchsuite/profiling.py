"""Model complexity profiler: DAG nodes, distinct symbols, operation histogram.

Raw size (sexpr length) can mislead — a tactic may grow the model yet speed it up
by trading expensive ops (distinct, ≠, store) for cheap ones. The op histogram is
the real signal. `profile()` fingerprints a goal; `diff()` compares two.
"""
import z3


def profile(goal):
    seen, ops, symbols = set(), {}, set()
    stack = [goal[i] for i in range(len(goal))]
    while stack:
        e = stack.pop()
        eid = e.get_id()
        if eid in seen:
            continue
        seen.add(eid)
        if z3.is_app(e):
            d = e.decl()
            ops[d.name()] = ops.get(d.name(), 0) + 1
            if d.kind() == z3.Z3_OP_UNINTERPRETED:
                symbols.add(d.name())
            stack.extend(e.children())
    return {"formulas": len(goal), "dag_nodes": len(seen),
            "symbols": len(symbols), "sexpr": len(goal.sexpr()), "ops": ops}


def diff(p0, p1, top=16):
    """Return (scalar deltas, op-histogram movers) between two profiles."""
    scalars = {k: (p0[k], p1[k], p1[k] - p0[k])
               for k in ("formulas", "dag_nodes", "symbols", "sexpr")}
    keys = set(p0["ops"]) | set(p1["ops"])
    movers = sorted(((k, p0["ops"].get(k, 0), p1["ops"].get(k, 0)) for k in keys),
                    key=lambda x: -abs(x[2] - x[1]))
    movers = [(k, b, a) for k, b, a in movers if b != a][:top]
    return scalars, movers
