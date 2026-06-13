"""Model complexity profiler — original goal vs tactic-produced goal.

Measures Z3 'complexity' three ways and diffs before/after a tactic chain:
  - DAG nodes  : unique AST nodes (the real model size; sharing-aware)
  - symbols    : distinct declared uninterpreted consts/functions (free vars)
  - op histogram : how many of each operator (and/or/ite/+/select/store/set.*/…)
Optionally exports both SMT2 files.

Usage:
  python3 complexity.py --task dispatch --encoding set --scale 200
  python3 complexity.py --task dispatch --encoding ite --scale 200 \
      --tactics simplify,propagate-values,solve-eqs,simplify
  python3 complexity.py --task coloring --encoding int --scale 60 --export /tmp/cx
"""
import argparse
import z3
from suite_tasks import TASKS
from suite import apply_combo


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


def show(task, enc, N, tactics, export):
    theories, build, expected = TASKS[task]["encodings"][enc]
    g0 = build(N)
    p0 = profile(g0)
    g1, tac_ms, err = apply_combo(g0, tuple(tactics))
    if err:
        print(f"tactic chain failed: {err}")
        return
    p1 = profile(g1)

    print(f"# {task}/{enc}  N={N}  theories={'+'.join(theories)}")
    print(f"  tactic chain: {' > '.join(tactics) or '(none)'}   (apply {tac_ms:.1f} ms)\n")
    print(f"  {'metric':12} {'before':>10} {'after':>10} {'Δ':>10}")
    for k in ("formulas", "dag_nodes", "symbols", "sexpr"):
        b, a = p0[k], p1[k]
        print(f"  {k:12} {b:>10} {a:>10} {a-b:>+10}")

    # op histogram diff (top movers)
    keys = set(p0["ops"]) | set(p1["ops"])
    diffs = sorted(((k, p0["ops"].get(k, 0), p1["ops"].get(k, 0)) for k in keys),
                   key=lambda x: -abs(x[2] - x[1]))
    print(f"\n  {'operation':22} {'before':>8} {'after':>8} {'Δ':>8}")
    for name, b, a in diffs[:16]:
        if b == a:
            continue
        print(f"  {name:22} {b:>8} {a:>8} {a-b:>+8}")

    if export:
        import os
        os.makedirs(export, exist_ok=True)
        base = f"{export}/{task}_{enc}_{N}"
        open(base + "_before.smt2", "w").write(g0.sexpr())
        open(base + "_after.smt2", "w").write(g1.sexpr())
        print(f"\n  exported {base}_{{before,after}}.smt2")


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--task", required=True)
    ap.add_argument("--encoding", required=True)
    ap.add_argument("--scale", type=int, required=True)
    ap.add_argument("--tactics", default="simplify,propagate-values,solve-eqs,simplify",
                    help="comma-separated tactic chain (default: the t01 PIPE)")
    ap.add_argument("--export", metavar="DIR")
    a = ap.parse_args()
    tactics = [t for t in a.tactics.split(",") if t]
    show(a.task, a.encoding, a.scale, tactics, a.export)
