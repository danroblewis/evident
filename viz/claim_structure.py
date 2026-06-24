"""claim_structure.py — the ABSTRACT solution-space STRUCTURE of a claim (Z3).

Beyond the bare per-variable ranges claim_space / solution_space show, this decomposes a claim
into what it DETERMINES vs leaves free — pure Z3 over the claim body, no sampling:

  * BACKBONE   — variables forced to a single value in EVERY solution (claim ∧ v ≠ v0 is UNSAT).
  * FREE       — the remaining variables, with their proven [lo, hi] ranges (Optimize).
  * EQUALITIES — pairs of (free) variables forced equal in every solution (claim ∧ a ≠ b UNSAT).
  * INEQUALITIES — pairs forced DIFFERENT in every solution (claim ∧ a = b UNSAT — e.g. XOR's a≠b).

  solution_structure(smt2_path, schema_path) -> {"sat", "backbone", "free", "equalities", "inequalities"}
"""
import sympy
import z3

from render_claim_space import _load_claim, _opt_bound

_SCALAR = ("int", "real", "bool", "enum")


def _fmt_relation(coeffs, const, names):
    """Render an integer affine relation Σ cᵢ·varᵢ = const as 'lhs = rhs' (positive terms left,
    negatives + the constant right): a+b-c=0 → 'a + b = c'; a-b=3 → 'a = b + 3'; a-b-d=-10 → 'a = b + d - 10'."""
    pos, neg = [], []
    for c, n in zip(coeffs, names):
        if c == 0:
            continue
        term = n if abs(c) == 1 else f"{abs(c)}·{n}"
        (pos if c > 0 else neg).append(term)
    lhs = " + ".join(pos) if pos else "0"
    rhs = " + ".join(neg)
    if const > 0:
        rhs = f"{rhs} + {const}" if rhs else str(const)
    elif const < 0:
        rhs = f"{rhs} - {-const}" if rhs else str(const)
    return f"{lhs} = {rhs or '0'}"


def _nonpairwise(body, consts, names):
    """IMPLIED affine relations among the free INTEGER vars beyond pairwise (a+b=c, a=b+3): sample
    solutions, take the EXACT rational null space (sympy) of the sampled points — exact so ≥2 co-existing
    relations stay clean integer vectors (#337) — then VERIFY each candidate via Z3 (body ∧ relation≠const
    UNSAT) so a sampling coincidence is never reported. Skips pure pairwise (the equalities pass owns a=b).
    Returns equation strings."""
    sol = z3.Solver(); sol.add(body)
    pts = []
    for _ in range(len(names) + 4):
        if sol.check() != z3.sat:
            break
        m = sol.model()
        pts.append([m.eval(consts[n], model_completion=True).as_long() for n in names])
        sol.add(z3.Or(*[consts[n] != m.eval(consts[n], model_completion=True) for n in names]))
    if len(pts) < 2:
        return []
    V = sympy.Matrix(pts)
    v0 = V.row(0)
    diffs = sympy.Matrix([V.row(i) - v0 for i in range(1, V.rows)])
    out = []
    for w in diffs.nullspace():                           # EXACT rational null space — ≥2 co-existing
        w = w * sympy.lcm([t.q for t in w])               # relations stay clean integer vectors (#337)
        ints = [int(x) for x in w]
        if not any(ints):
            continue
        if next(x for x in ints if x) < 0:                # sign-normalize: leading coeff positive (stable)
            ints = [-x for x in ints]
        const = int((sympy.Matrix([ints]) * v0.T)[0])
        if sum(1 for x in ints if x) < 3 and const == 0:  # pure pairwise a=b — equalities handles it
            continue
        expr = z3.Sum([ints[i] * consts[names[i]] for i in range(len(ints))])
        s2 = z3.Solver(); s2.add(body); s2.add(expr != const)
        if s2.check() == z3.unsat:
            out.append(_fmt_relation(ints, const, names))
    return out


def solution_structure(smt2_path, schema_path):
    sch, body, consts = _load_claim(smt2_path, schema_path)
    vars_ = [v for v in sch.get("vars", [])
             if v["name"] in consts and v["kind"] in _SCALAR]
    sol = z3.Solver(); sol.add(body)
    if sol.check() != z3.sat:
        return {"sat": False, "backbone": [], "free": [], "equalities": [], "inequalities": [], "relations": []}
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
    eqs, ineqs = [], []
    for i in range(len(freev)):
        for j in range(i + 1, len(freev)):
            if freev[i]["kind"] != freev[j]["kind"]:
                continue
            ci, cj = consts[freev[i]["name"]], consts[freev[j]["name"]]
            s = z3.Solver(); s.add(body); s.add(ci != cj)
            if s.check() == z3.unsat:
                eqs.append((freev[i]["name"], freev[j]["name"]))
                continue
            s2 = z3.Solver(); s2.add(body); s2.add(ci == cj)   # forced DIFFERENT in every solution
            if s2.check() == z3.unsat:
                ineqs.append((freev[i]["name"], freev[j]["name"]))
    free_int = [v["name"] for v in freev if v["kind"] == "int"]
    relations = _nonpairwise(body, consts, free_int) if len(free_int) >= 3 else []
    return {"sat": True, "backbone": backbone, "free": free, "equalities": eqs,
            "inequalities": ineqs, "relations": relations}
