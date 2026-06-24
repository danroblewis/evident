"""claim_structure.py — the ABSTRACT solution-space STRUCTURE of a claim (Z3).

Beyond the bare per-variable ranges claim_space / solution_space show, this decomposes a claim
into what it DETERMINES vs leaves free — pure Z3 over the claim body, no sampling:

  * BACKBONE   — variables forced to a single value in EVERY solution (claim ∧ v ≠ v0 is UNSAT).
  * FREE       — the remaining variables, with their proven [lo, hi] ranges (Optimize).
  * EQUALITIES — pairs of (free) variables forced equal in every solution (claim ∧ a ≠ b UNSAT).

  solution_structure(smt2_path, schema_path) -> {"sat", "backbone", "free", "equalities"}
"""
import z3

from render_claim_space import _load_claim, _opt_bound

_SCALAR = ("int", "real", "bool", "enum")


def solution_structure(smt2_path, schema_path):
    sch, body, consts = _load_claim(smt2_path, schema_path)
    vars_ = [v for v in sch.get("vars", [])
             if v["name"] in consts and v["kind"] in _SCALAR]
    sol = z3.Solver(); sol.add(body)
    if sol.check() != z3.sat:
        return {"sat": False, "backbone": [], "free": [], "equalities": []}
    mdl = sol.model()

    backbone, free = [], []
    for v in vars_:
        c = consts[v["name"]]
        v0 = mdl.eval(c, model_completion=True)
        s = z3.Solver(); s.add(body); s.add(c != v0)
        if s.check() == z3.unsat:
            backbone.append((v["name"], str(v0)))
        else:
            rng = (_opt_bound(body, c, False), _opt_bound(body, c, True)) \
                if v["kind"] in ("int", "real") else None
            free.append((v["name"], rng))

    free_names = {n for n, _ in free}
    freev = [v for v in vars_ if v["name"] in free_names]
    eqs = []
    for i in range(len(freev)):
        for j in range(i + 1, len(freev)):
            if freev[i]["kind"] != freev[j]["kind"]:
                continue
            ci, cj = consts[freev[i]["name"]], consts[freev[j]["name"]]
            s = z3.Solver(); s.add(body); s.add(ci != cj)
            if s.check() == z3.unsat:
                eqs.append((freev[i]["name"], freev[j]["name"]))
    return {"sat": True, "backbone": backbone, "free": free, "equalities": eqs}
