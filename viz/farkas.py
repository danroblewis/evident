"""farkas.py — certificates for IMPLIED affine relations beyond the plain equality combo.

claim_structure.py finds a relation `r(x) = R` forced by a claim and tries `_farkas_combo` —
the integer/rational linear combination of the EQUALITY core constraints that derives it. That
combo is None whenever an INEQUALITY does the forcing (`2·a=c` from `c=a+b ∧ a−b≤0 ∧ b−a≤0`;
`a=4` from `a≤4 ∧ a≥4`). This module supplies the two things that case needs:

  * motzkin_certificate(...) — (#348) the FARKAS / MOTZKIN certificate: NON-NEGATIVE multipliers
    λ≥0 over the inequalities (+ free multipliers over equalities) that pin the relation from BOTH
    sides — r≤R via one combination, r≥R via another, together forcing r=R. Solved as an LP
    feasibility (Z3 LRA, sparsest), then RECONSTRUCTED and checked so a wrong certificate is
    impossible (the combination must equal the goal affine form exactly).

  * lattice_relations(...) — (#350) the integer null-space LATTICE, not just sympy's sparse basis.
    `.nullspace()` returns `2x=y` and `2z=y` but never the 3-var `x+z=y` (a fractional combo of the
    basis). Enumerate small-coefficient combinations of the basis, primitive- + sign-normalize, and
    rank by (fewest variables, smallest coefficient) so the genuinely minimal relations surface
    without flooding. Each candidate is still Z3-verified by the caller — this only proposes.
"""
import itertools
from math import gcd

import sympy
import z3


def _affine(expr, consts, names, is_real):
    """A linear z3 arithmetic expr → (coeffs over `names`, const) as exact sympy numbers, by the same
    substitution trick `_coef_vec` uses. None if a coefficient isn't an exact number (nonlinear)."""
    val = z3.RealVal if is_real else z3.IntVal

    def ev(assign):
        e = z3.simplify(z3.substitute(expr, *[(consts[n], val(assign.get(n, 0))) for n in names]))
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
    return coefs, base


def _normalize_atom(cc, consts, names, is_real):
    """A core atom → ('eq'|'le', coeffs, const) meaning `Σ coeffs·var + const {== | ≤} 0`. Equalities
    stay 'eq' (free multiplier); every inequality is folded to the canonical `≤ 0` (λ≥0). Strict
    `<`/`>` fold to `≤` — sound for deriving the non-strict goal. None if the atom isn't linear."""
    k = cc.decl().kind()
    af = _affine(cc.arg(0) - cc.arg(1), consts, names, is_real)
    if af is None:
        return None
    cf, b0 = af
    if k == z3.Z3_OP_EQ:
        return ("eq", cf, b0)
    if k in (z3.Z3_OP_LE, z3.Z3_OP_LT):
        return ("le", cf, b0)
    if k in (z3.Z3_OP_GE, z3.Z3_OP_GT):
        return ("le", [-x for x in cf], -b0)
    return None


def _solve_direction(rows, goal_coeffs, goal_const, n_names):
    """λ with λ≥0 on 'le' rows, free on 'eq' rows, s.t. Σλᵢ·rowᵢ == goal (coeffs + const) EXACTLY.
    Z3 LRA feasibility, minimizing Σ|λ| for the sparsest/cleanest certificate. List of sympy λ, or
    None if infeasible (the goal isn't a non-negative combination of these rows in this direction)."""
    s = z3.Optimize()
    lam = [z3.Real(f"__l{i}") for i in range(len(rows))]
    absl = [z3.Real(f"__al{i}") for i in range(len(rows))]
    for i, (kind, _, _) in enumerate(rows):
        if kind == "le":
            s.add(lam[i] >= 0)
        s.add(absl[i] >= lam[i], absl[i] >= -lam[i])
    for vi in range(n_names):
        s.add(z3.Sum([lam[i] * z3.RealVal(str(rows[i][1][vi])) for i in range(len(rows))])
              == z3.RealVal(str(goal_coeffs[vi])))
    s.add(z3.Sum([lam[i] * z3.RealVal(str(rows[i][2])) for i in range(len(rows))])
          == z3.RealVal(str(goal_const)))
    s.minimize(z3.Sum(absl))
    if s.check() != z3.sat:
        return None
    m = s.model()
    out = []
    for i in range(len(rows)):
        v = m.eval(lam[i], model_completion=True)
        out.append(sympy.Rational(v.numerator_as_long(), v.denominator_as_long())
                   if z3.is_rational_value(v) else sympy.Integer(v.as_long()))
    return out


def _reconstruct_ok(rows, lam, goal_coeffs, goal_const, n_names):
    """SELF-CHECK: Σλᵢ·rowᵢ must equal the goal affine form EXACTLY, and every 'le' λ must be ≥0.
    A certificate that fails this is never emitted — the anti-lie guard for the Motzkin path (#348)."""
    for i, (kind, _, _) in enumerate(rows):
        if kind == "le" and lam[i] < 0:
            return False
    for vi in range(n_names):
        if sum(lam[i] * rows[i][1][vi] for i in range(len(rows))) != goal_coeffs[vi]:
            return False
    return sum(lam[i] * rows[i][2] for i in range(len(rows))) == goal_const


def _fmt_side(rows, lam, core_strs, op):
    """One direction of the certificate: the λ-weighted constraint list + the `≤`/`≥` it proves."""
    parts = []
    for i, l in enumerate(lam):
        if l == 0:
            continue
        mag = "" if abs(l) == 1 else f"{abs(l)}·"
        sign = "" if not parts and l > 0 else (" + " if l > 0 else " − ")
        parts.append(f"{sign}{mag}({core_strs[i]})")
    return "".join(parts), op


def motzkin_certificate(core_objs, core_strs, consts, names, rel_ints, rel_const, is_real):
    """#348 — the Farkas/Motzkin certificate pinning `Σ rel_ints·var = rel_const` from BOTH sides via
    non-negative multipliers over the (in)equalities. Returns a display string (e.g. 'r ≤ R via … and
    r ≥ R via …'), or None if no clean certificate exists for either side (caller keeps the bare core).
    Reconstruction-checked, so a returned certificate is sound by construction."""
    rows = [_normalize_atom(cc, consts, names, is_real) for cc in core_objs]
    if any(r is None for r in rows):
        return None
    n = len(names)
    # goal g(x) = r(x) − R: coeffs = rel_ints, const = −R.  Prove g ≤ 0 (r ≤ R) and −g ≤ 0 (r ≥ R).
    gc = [sympy.Integer(x) for x in rel_ints]
    gk = -sympy.sympify(rel_const)
    up = _solve_direction(rows, gc, gk, n)                       # r ≤ R
    if up is None or not _reconstruct_ok(rows, up, gc, gk, n):
        return None
    dn = _solve_direction(rows, [-x for x in gc], -gk, n)        # r ≥ R
    if dn is None or not _reconstruct_ok(rows, dn, [-x for x in gc], -gk, n):
        return None
    up_s, _ = _fmt_side(rows, up, core_strs, "≤")
    dn_s, _ = _fmt_side(rows, dn, core_strs, "≥")
    if not up_s and not dn_s:
        return None
    return f"{up_s or '0'}  pins  ≤ ;  {dn_s or '0'}  pins  ≥  — together forcing the equality"


def _primitive(v):
    """An integer vector → its primitive form (divide out the gcd), sign-normalized so the leading
    nonzero coefficient is positive. None for the zero vector."""
    ints = [int(x) for x in v]
    if not any(ints):
        return None
    g = 0
    for x in ints:
        g = gcd(g, abs(x))
    ints = [x // g for x in ints]
    if next(x for x in ints if x) < 0:
        ints = [-x for x in ints]
    return tuple(ints)


def _enumerate(basis, span):
    """Every primitive integer vector in the lattice spanned by `basis`, from small-coefficient
    combinations (coefficients in [−span, span]). Deduped, ranked CLEANEST first — by (max coefficient,
    then fewest nonzero vars)."""
    ibasis = [b * sympy.lcm([t.q for t in b]) for b in basis]   # clear rational basis to integers
    seen, out = set(), []
    for combo in itertools.product(range(-span, span + 1), repeat=len(ibasis)):
        if not any(combo):
            continue
        v = sympy.zeros(ibasis[0].rows, 1)
        for c, b in zip(combo, ibasis):
            v += c * b
        pv = _primitive(v)
        if pv is None or pv in seen:
            continue
        seen.add(pv)
        out.append(pv)
    out.sort(key=lambda p: (max(abs(x) for x in p), sum(1 for x in p if x)))
    return out


def lattice_relations(basis, span=2):
    """#350 — the genuinely MINIMAL integer relations in the null-space lattice, not just sympy's sparse
    `.nullspace()` basis. `2x=y ∧ 2z=y` has null-space basis `2x=y`/`2z=y` (max-coeff 2) but the cleaner
    `x=z` and `x+z=y` (max-coeff 1) are fractional combinations the basis never surfaces. Enumerate small
    combinations, pick the cleanest independent basis (smallest max-coefficient first), then keep every
    candidate whose max-coefficient doesn't EXCEED that basis's — so the equally-clean alternatives
    surface while larger-coefficient derived relations (pure noise) are dropped. Returns relation-
    direction vectors (lists); the caller computes each const and Z3-verifies — this only proposes."""
    if not basis:
        return []
    cands = _enumerate(basis, span)
    if not cands:
        return []
    kept, M = [], sympy.zeros(0, len(cands[0]))
    for pv in cands:                                            # cleanest independent basis
        t = M.col_join(sympy.Matrix([pv]))
        if t.rank() > M.rank():
            kept.append(pv)
            M = t
    cap = max(max(abs(x) for x in p) for p in kept)            # don't exceed the cleanest generators
    return [list(p) for p in cands if max(abs(x) for x in p) <= cap]
