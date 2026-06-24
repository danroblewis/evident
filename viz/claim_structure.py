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


def _val(m, c):
    """A Z3 model value as an exact sympy number — Integer or Rational (#339 reals). None for an
    algebraic/irrational value (a linear claim never produces one; bail rather than approximate)."""
    v = m.eval(c, model_completion=True)
    if z3.is_int_value(v):
        return sympy.Integer(v.as_long())
    if z3.is_rational_value(v):
        return sympy.Rational(v.numerator_as_long(), v.denominator_as_long())
    return None


def conjuncts(e):
    """Flatten an And tree into its leaf assertions — the claim's individual constraints (for the core)."""
    if z3.is_app(e) and e.decl().kind() == z3.Z3_OP_AND:
        out = []
        for c in e.children():
            out.extend(conjuncts(c))
        return out
    return [e]


def _verify_core(body, relation_expr, rhs):
    """Verify the relation holds (body ∧ relation≠rhs UNSAT) AND extract the unsat core — the minimal claim
    constraints that FORCE the relation (#341, the interrogable proof). assert_and_track each body conjunct
    + the negated relation; the core's tracked constraints are the forcing ones. None if not UNSAT."""
    s = z3.Solver()
    tracked = {}
    for k, c in enumerate(conjuncts(body)):
        p = z3.Bool(f"__c{k}"); tracked[p.get_id()] = c
        s.assert_and_track(c, p)
    pn = z3.Bool("__neg"); tracked[pn.get_id()] = None
    s.assert_and_track(relation_expr != rhs, pn)
    if s.check() != z3.unsat:
        return None
    return [str(tracked[p.get_id()]) for p in s.unsat_core() if tracked.get(p.get_id()) is not None]


def _nonpairwise(body, consts, names, is_real):
    """IMPLIED affine relations among the free numeric vars beyond pairwise (a+b=c, a=b+3, and for reals
    y=2x): sample solutions, take the EXACT rational null space (sympy) of the sampled points — exact so ≥2
    co-existing relations stay clean integer vectors (#337) — then VERIFY each candidate via Z3 (body ∧
    relation≠const UNSAT) so a sampling coincidence is never reported. Handles int + real vars (#339).
    Skips pure pairwise (the equalities pass owns a=b). Returns {"eq", "core"} dicts — the relation string
    + the claim constraints that force it (#341, the interrogable proof)."""
    sol = z3.Solver(); sol.add(body)
    pts = []
    for _ in range(len(names) + 4):
        if sol.check() != z3.sat:
            break
        m = sol.model()
        row = [_val(m, consts[n]) for n in names]
        if any(x is None for x in row):                   # algebraic value — can't do an exact null space
            break
        pts.append(row)
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
        const = (sympy.Matrix([ints]) * v0.T)[0]          # sympy Integer (int claim) or Rational (real)
        if sorted(c for c in ints if c) == [-1, 1] and const == 0:  # EXACTLY a=b — equalities pass owns
            continue                                                # it (but keep scaling y=2x, [2,-1])
        expr = z3.Sum([ints[i] * consts[names[i]] for i in range(len(ints))])
        rhs = z3.RealVal(str(const)) if is_real else int(const)
        core = _verify_core(body, expr, rhs)              # verify + the constraints that force it (#341)
        if core is not None:
            out.append({"eq": _fmt_relation(ints, const, names), "core": core})
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
    free_num = [v["name"] for v in freev if v["kind"] in ("int", "real")]
    is_real = any(v["kind"] == "real" for v in freev if v["name"] in free_num)
    relations = _nonpairwise(body, consts, free_num, is_real) if len(free_num) >= 3 else []
    return {"sat": True, "backbone": backbone, "free": free, "equalities": eqs,
            "inequalities": ineqs, "relations": relations}
