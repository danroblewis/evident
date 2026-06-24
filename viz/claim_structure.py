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

from farkas import lattice_relations, motzkin_certificate
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
    + the negated relation; the core's tracked constraints are the forcing ones, made PROVABLY MINIMAL by
    a deletion pass (#344). None if not UNSAT."""
    s = z3.Solver()
    tracked = {}
    for k, c in enumerate(conjuncts(body)):
        p = z3.Bool(f"__c{k}"); tracked[p.get_id()] = c
        s.assert_and_track(c, p)
    pn = z3.Bool("__neg"); tracked[pn.get_id()] = None
    s.assert_and_track(relation_expr != rhs, pn)
    if s.check() != z3.unsat:
        return None
    core = [tracked[p.get_id()] for p in s.unsat_core() if tracked.get(p.get_id()) is not None]
    # #344: Z3's unsat_core() returns *an* unsat core, not necessarily MINIMAL. Make it provably minimal by
    # DELETION — drop each constraint and keep it only if the relation is still forced without it. So every
    # constraint the proof cites is load-bearing (matching Z3's own minimize-on-cores idiom).
    i = 0
    while i < len(core):
        s2 = z3.Solver(); s2.add(relation_expr != rhs)
        for c in core[:i] + core[i + 1:]:
            s2.add(c)
        if s2.check() == z3.unsat:
            del core[i]                                    # constraint i was redundant — drop it
        else:
            i += 1
    return [str(c) for c in core]


def _coef_vec(constraint, consts, names, is_real):
    """A linear equality `lhs == rhs` → (coeffs over names, rhs) as exact sympy numbers. Extract by
    substitution: coef of a var = (lhs−rhs at that var=1, rest 0) − (at all 0); the constant gives the rhs.
    Handles int + real (#347, rational coeffs). None if a value isn't an exact number (nonlinear)."""
    diff = constraint.arg(0) - constraint.arg(1)          # constraint is diff == 0
    val = z3.RealVal if is_real else z3.IntVal

    def ev(assign):
        e = z3.simplify(z3.substitute(diff, *[(consts[n], val(assign.get(n, 0))) for n in names]))
        if z3.is_int_value(e):
            return sympy.Integer(e.as_long())
        if z3.is_rational_value(e):
            return sympy.Rational(e.numerator_as_long(), e.denominator_as_long())
        return None
    base = ev({})
    if base is None:
        return None
    coefs = []
    for n in names:
        c = ev({n: 1})
        if c is None:
            return None
        coefs.append(c - base)
    return coefs, -base


def _fmt_combo(combo):
    """[(λ, constraint_str)] → a signed linear-combination string: '(c1) − 2·(c2)'."""
    parts = []
    for i, (lam, c) in enumerate(combo):
        sign = "" if i == 0 and lam > 0 else (" + " if lam > 0 else " − ")
        mag = "" if abs(lam) == 1 else f"{abs(lam)}·"
        parts.append(f"{sign}{mag}({c})")
    return "".join(parts)


def _farkas_combo(body, consts, names, core_strs, rel_ints, rel_const, is_real):
    """The integer linear combination of the core constraints that DERIVES the relation (#345, the Farkas
    certificate — 'how' the constraints force it, not just 'which'). Solve Σλⱼ·constraintⱼ = relation per
    variable, then check the constant. Int + real / rational λ (#347); None if underdetermined, a constraint
    isn't a linear equality, or the const mismatches (then the caller shows the bare core list)."""
    try:
        by_str = {str(c): c for c in conjuncts(body)}
        cores = [by_str[s] for s in core_strs if s in by_str]
        vecs = [_coef_vec(c, consts, names, is_real) for c in cores]
        if not cores or any(v is None for v in vecs):                     # an inequality / nonlinear core
            return None
        m = sympy.Matrix([[v[0][i] for v in vecs] for i in range(len(names))])
        sol = sympy.linsolve((m, sympy.Matrix(rel_ints)))
        if not sol:
            return None
        lam = list(list(sol)[0])
        if any(getattr(x, "free_symbols", set()) for x in lam):           # underdetermined
            return None
        if sum(lam[j] * vecs[j][1] for j in range(len(vecs))) != rel_const:
            return None
        used = [(lam[j], core_strs[j]) for j in range(len(cores)) if lam[j] != 0]
        used.sort(key=lambda x: bool(x[0] < 0))                           # positive terms first (cleaner)
        return _fmt_combo(used) if used else None
    except Exception:
        return None


def _smtlib_obligation(body, consts, relation_expr, rhs):
    """The relation's proof obligation as named-assert SMT-LIB (#346/#349): the claim constraints + the
    NEGATED relation + (check-sat)(get-unsat-core). Paste into z3 — UNSAT proves the relation, the unsat
    core names the forcing constraints, so the interrogation can leave the IDE into your own solver."""
    decls = "\n".join(f"(declare-fun {c} () {c.sort().sexpr()})" for c in consts.values())
    asserts = "\n".join(f"(assert (! {c.sexpr()} :named c{i}))" for i, c in enumerate(conjuncts(body)))
    return (f"(set-option :produce-unsat-cores true)\n{decls}\n{asserts}\n"
            f"(assert (! {z3.Not(relation_expr == rhs).sexpr()} :named goal))\n(check-sat)\n(get-unsat-core)")


def _emit_relation(body, consts, names, ints, const, is_real):
    """One candidate integer relation `Σ ints·var = const` → its emitted dict, or None if Z3 can't
    confirm it's forced (a sampling/lattice coincidence). Carries the unsat-core proof (#341), the
    equality Farkas derivation (#345) OR — when an inequality does the forcing — the Motzkin/Farkas
    λ≥0 certificate (#348), and the SMT-LIB obligation (#346/#349)."""
    if sorted(c for c in ints if c) == [-1, 1] and const == 0:    # EXACTLY a=b — the equalities pass
        return None                                               # owns it (but keep scaling y=2x, [2,-1])
    expr = z3.Sum([ints[i] * consts[names[i]] for i in range(len(ints))])
    rhs = z3.RealVal(str(const)) if is_real else int(const)
    core = _verify_core(body, expr, rhs)                          # verify + forcing constraints (#341)
    if core is None:
        return None
    combo = _farkas_combo(body, consts, names, core, ints, const, is_real)   # equality derivation (#345)
    rec = {"eq": _fmt_relation(ints, const, names), "core": core, "combo": combo,
           "smtlib": _smtlib_obligation(body, consts, expr, rhs)}            # #346/#349 export
    if combo is None:                                             # #348: an inequality forces it — the
        by_str = {str(c): c for c in conjuncts(body)}             # equality combo is None, so reach for
        core_objs = [by_str[s] for s in core if s in by_str]      # the Farkas/Motzkin λ≥0 certificate.
        if len(core_objs) == len(core):
            rec["motzkin"] = motzkin_certificate(core_objs, core, consts, names, ints, const, is_real)
    return rec


def _nonpairwise(body, consts, names, is_real):
    """IMPLIED affine relations among the free numeric vars beyond pairwise (a+b=c, a=b+3, and for reals
    y=2x): sample solutions, take the EXACT rational null space (sympy) of the sampled points — exact so ≥2
    co-existing relations stay clean integer vectors (#337) — then enumerate the SHORT integer vectors in
    that null-space LATTICE (#350, so x+z=y surfaces alongside 2x=y/2z=y, not just sympy's sparse basis)
    and VERIFY each via Z3 (body ∧ relation≠const UNSAT) so a sampling/lattice coincidence is never
    reported. Handles int + real vars (#339). Skips pure pairwise (the equalities pass owns a=b). Returns
    {"eq", "core", "combo"/"motzkin", "smtlib"} dicts — the relation + its interrogable proof (#341)."""
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
    out, seen = [], set()
    for ints in lattice_relations(diffs.nullspace()):     # #350 short lattice vectors, ranked low-var-first
        const = (sympy.Matrix([ints]) * v0.T)[0]          # sympy Integer (int claim) or Rational (real)
        rec = _emit_relation(body, consts, names, ints, const, is_real)
        if rec and rec["eq"] not in seen:                 # dedupe by the rendered relation (sign-normalized)
            seen.add(rec["eq"]); out.append(rec)
    return out


def _ineq_cert(body, consts, names, ints, const, is_real):
    """#348 — the Motzkin/Farkas λ≥0 certificate for a fact pinned by INEQUALITIES: a backbone (a=4 from
    a≤4 ∧ a≥4) or an equality (a=b from a−b≤0 ∧ b−a≤0). _emit_relation already supplies this for the
    non-pairwise RELATIONS; this extends it to the backbones + equalities that #348's own examples hit.
    None when the fact is equality-forced (no inequality in the core) or unverifiable. Reconstruction-
    checked inside motzkin_certificate, so a returned cert is sound by construction."""
    expr = z3.Sum([ints[i] * consts[names[i]] for i in range(len(ints))])
    rhs = z3.RealVal(str(const)) if is_real else int(const)
    core = _verify_core(body, expr, rhs)
    if core is None or _farkas_combo(body, consts, names, core, ints, const, is_real) is not None:
        return None                                              # unverifiable, or purely equality-forced
    by_str = {str(c): c for c in conjuncts(body)}
    core_objs = [by_str[s] for s in core if s in by_str]
    if len(core_objs) != len(core):
        return None
    return motzkin_certificate(core_objs, core, consts, names, ints, const, is_real)


def solution_structure(smt2_path, schema_path):
    sch, body, consts = _load_claim(smt2_path, schema_path)
    vars_ = [v for v in sch.get("vars", [])
             if v["name"] in consts and v["kind"] in _SCALAR]
    sol = z3.Solver(); sol.add(body)
    if sol.check() != z3.sat:
        return {"sat": False, "backbone": [], "free": [], "equalities": [], "inequalities": [],
                "relations": [], "forced_certs": []}
    mdl = sol.model()

    backbone, free, forced_certs = [], [], []
    for v in vars_:
        c = consts[v["name"]]
        v0 = mdl.eval(c, model_completion=True)
        s = z3.Solver(); s.add(body); s.add(c != v0)
        if s.check() == z3.unsat:
            backbone.append((v["name"], str(v0)))
            if z3.is_int_value(v0):                             # #348: INEQUALITIES pin the value → show the cert
                cert = _ineq_cert(body, consts, [v["name"]], [1], v0.as_long(), False)
                if cert:
                    forced_certs.append({"what": f"{v['name'].split('.')[-1]} = {v0}", "cert": cert})
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
                ec = _ineq_cert(body, consts, [freev[i]["name"], freev[j]["name"]], [1, -1], 0,
                                freev[i]["kind"] == "real")      # #348: a=b pinned by a≤b ∧ b≤a
                if ec:
                    forced_certs.append({"what": f"{freev[i]['name'].split('.')[-1]} = {freev[j]['name'].split('.')[-1]}", "cert": ec})
                continue
            s2 = z3.Solver(); s2.add(body); s2.add(ci == cj)   # forced DIFFERENT in every solution
            if s2.check() == z3.unsat:
                ineqs.append((freev[i]["name"], freev[j]["name"]))
    free_num = [v["name"] for v in freev if v["kind"] in ("int", "real")]
    is_real = any(v["kind"] == "real" for v in freev if v["name"] in free_num)
    relations = _nonpairwise(body, consts, free_num, is_real) if len(free_num) >= 3 else []
    return {"sat": True, "backbone": backbone, "free": free, "equalities": eqs,
            "inequalities": ineqs, "relations": relations, "forced_certs": forced_certs}
