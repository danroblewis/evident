#!/usr/bin/env python3
"""Phase 2.23 — ABSTRACT claim solution-structure (viz/claim_structure.py + render).

Pins the backbone / free / implied-equality decomposition (pure Z3 over the claim body): a claim
whose constraints FORCE variables (a+b=10 ∧ a−b=4 ⇒ a=7, b=3) surfaces them as the backbone; a
claim that couples variables (y=x) surfaces the implied equality; an under-constrained claim leaves
them free. This is the new analysis claims get beyond the bare ranges of claim_space.

Run from repo root: python3 ide/test_solution_structure.py
"""
import sys
import tempfile

sys.path.insert(0, "ide/web")
sys.path.insert(0, "viz")

from runtime_io import _export                          # noqa: E402
from claim_structure import solution_structure          # noqa: E402

CASES = [
    ("sys (a,b forced; c free)",
     "claim sys\n    a, b ∈ Int\n    0 ≤ c ∈ Int ≤ 5\n    a + b = 10\n    a - b = 4",
     {"a", "b"}, {"c"}, set()),
    ("coupled (x,y free but forced equal)",
     "claim coupled\n    0 ≤ x ∈ Int ≤ 10\n    0 ≤ y ∈ Int ≤ 10\n    y = x",
     set(), {"x", "y"}, {("x", "y")}),
    ("packing (both free)",
     "claim packing\n    0 ≤ x ∈ Int ≤ 20\n    0 ≤ y ∈ Int ≤ 20\n    y ≤ x\n    x + y ≤ 20",
     set(), {"x", "y"}, set()),
]


def _short(n):
    return n.split(".")[-1]


def _check_relations(fails):
    """#329/#337/#339/#341 — non-pairwise implied relations + their forcing-constraint proof cores."""
    # #329 + #337: non-pairwise IMPLIED relations — single (a+b=c, affine a=b+3) AND ≥2 co-existing
    # (exact sympy null space, each Z3-verified, never a sampling coincidence).
    for src, want in [
        ("claim t\n    0 ≤ a ∈ Int ≤ 10\n    0 ≤ b ∈ Int ≤ 10\n    0 ≤ c ∈ Int ≤ 20\n    c = a + b", "a + b = c"),
        ("claim t\n    0 ≤ a ∈ Int ≤ 10\n    0 ≤ b ∈ Int ≤ 10\n    0 ≤ d ∈ Int ≤ 10\n    a = b + 3", "a = b + 3"),
    ]:
        with tempfile.TemporaryDirectory() as w:
            ok, prefix, *_ = _export(src, w)
            rel = [x["eq"] for x in solution_structure(prefix + ".smt2", prefix + ".schema.json").get("relations", [])]
            if want not in rel:
                fails.append(f"non-pairwise: {want!r} not in {rel}")
    # #337: TWO independent relations co-existing must BOTH surface (float SVD found 0; exact finds 2).
    with tempfile.TemporaryDirectory() as w:
        ok, prefix, *_ = _export("claim t\n    0 ≤ a ∈ Int ≤ 5\n    0 ≤ b ∈ Int ≤ 5\n    0 ≤ c ∈ Int ≤ 10\n"
                                 "    0 ≤ d ∈ Int ≤ 15\n    c = a + b\n    d = a - b + 10", w)
        rel = solution_structure(prefix + ".smt2", prefix + ".schema.json").get("relations", [])
        if len(rel) != 2:
            fails.append(f"#337 two-relation: expected 2 relations, got {rel}")
    # #339: REAL-typed relations — a scaling y=2x surfaces (NOT skipped as pairwise), Z3-verified over Real.
    with tempfile.TemporaryDirectory() as w:
        ok, prefix, *_ = _export("claim t\n    0.0 ≤ x ∈ Real ≤ 10.0\n    y ∈ Real\n    z ∈ Real\n"
                                 "    y = 2.0 * x\n    z = x + y", w)
        rels = solution_structure(prefix + ".smt2", prefix + ".schema.json").get("relations", [])
        if "2·x = y" not in [x["eq"] for x in rels]:
            fails.append(f"#339 real: '2·x = y' not in {[x['eq'] for x in rels]}")
        # #347: real-valued relations also carry their Farkas derivation (rational λ); 2·x=y derives from y==2*x.
        yx = next((x for x in rels if x["eq"] == "2·x = y"), None)
        if yx and not (yx.get("combo") and "y == 2*x" in yx["combo"]):
            fails.append(f"#347 real combo: '2·x = y' should derive from y==2*x, got {yx.get('combo')}")
    # #341: each relation carries its UNSAT-core proof. A DERIVED relation (a+b=10 ∧ b+c=12 together force
    # c=a+2) must cite BOTH forcing constraints in its core — the interrogable "why is this forced".
    with tempfile.TemporaryDirectory() as w:
        ok, prefix, *_ = _export("claim t\n    0 ≤ a ∈ Int ≤ 10\n    0 ≤ b ∈ Int ≤ 10\n    0 ≤ c ∈ Int ≤ 12\n"
                                 "    a + b = 10\n    b + c = 12", w)
        rels = solution_structure(prefix + ".smt2", prefix + ".schema.json").get("relations", [])
        forced = [x for x in rels if any("a + b" in cc for cc in x["core"]) and any("b + c" in cc for cc in x["core"])]
        if not forced:
            fails.append(f"#341: no relation cites both forcing constraints; got {[(x['eq'], x['core']) for x in rels]}")
        # #345: the DERIVED relation's Farkas combo derives it as a linear combination of BOTH constraints.
        elif not (forced[0].get("combo") and "a + b" in forced[0]["combo"] and "b + c" in forced[0]["combo"]):
            fails.append(f"#345: derived combo should cite both constraints, got {forced[0].get('combo')}")
        # #344: the core is provably MINIMAL — exactly the 2 forcing equalities, NO redundant bound (the
        # claim has 0≤a≤10 etc., but they don't force the relation, so the deletion pass drops them).
        if forced and (len(forced[0]["core"]) != 2 or any("<=" in cc for cc in forced[0]["core"])):
            fails.append(f"#344: core should be minimal (2 equalities, no bound), got {forced[0]['core']}")
        # #346/#349: the exported SMT-LIB obligation re-parses in z3 to UNSAT (so the user can re-derive it).
        if forced and forced[0].get("smtlib"):
            import z3
            zs = z3.Solver(); zs.set(unsat_core=True); zs.from_string(forced[0]["smtlib"])
            if zs.check() != z3.unsat:
                fails.append("#346: exported SMT-LIB obligation should be UNSAT (proving the relation)")


def _reverify_smtlib(rec):
    """A relation's exported SMT-LIB obligation must re-parse to UNSAT in z3 — the relation truly holds,
    so no emitted certificate (combo or Motzkin) can be a lie."""
    import z3
    zs = z3.Solver(); zs.set(unsat_core=True); zs.from_string(rec["smtlib"])
    return zs.check() == z3.unsat


def _check_inequality_forced(fails):
    """#348 — a relation forced by INEQUALITIES (no equality combo) carries the FARKAS/MOTZKIN certificate:
    λ≥0 multipliers over the inequalities that pin it from both sides. The bare combo is None there."""
    # x+y ≤ 10 ∧ x+y ≥ 10 together force x+y=10 — a relation with NO equality core. The Motzkin
    # certificate must cite both inequalities (≤ pins one side, ≥ the other); and it must Z3-re-verify.
    with tempfile.TemporaryDirectory() as w:
        ok, prefix, *_ = _export("claim t\n    0 ≤ x ∈ Int ≤ 10\n    0 ≤ y ∈ Int ≤ 10\n    0 ≤ z ∈ Int ≤ 10\n"
                                 "    x + y ≤ 10\n    x + y ≥ 10", w)
        rels = solution_structure(prefix + ".smt2", prefix + ".schema.json").get("relations", [])
        rec = next((r for r in rels if r["eq"] == "x + y = 10"), None)
        if rec is None:
            fails.append(f"#348: 'x + y = 10' not surfaced; got {[r['eq'] for r in rels]}")
        else:
            if rec.get("combo") is not None:
                fails.append(f"#348: inequality-forced relation should have NO equality combo, got {rec['combo']}")
            mz = rec.get("motzkin")
            if not (mz and "<=" in mz and ">=" in mz and "pins" in mz):
                fails.append(f"#348: Motzkin certificate should pin from both sides, got {mz!r}")
            if not _reverify_smtlib(rec):  # the emitted certificate's relation truly holds in z3
                fails.append("#348: inequality-forced relation's obligation should re-verify UNSAT in z3")
    # Unit re-verification of the certificate machinery on the task's exact named cases — including the
    # ANTI-LIE guard: an UNforced relation yields NO certificate (the reconstruction self-check rejects it).
    import z3
    sys.path.insert(0, "viz")
    from farkas import motzkin_certificate                  # noqa: E402
    a, b, c, x, y = z3.Ints("a b c x y")
    cases = [
        (([c == a + b, a - b <= 0, b - a <= 0], {"a": a, "b": b, "c": c}, ["a", "b", "c"], [2, 0, -1], 0), True),
        (([a <= 4, a >= 4], {"a": a}, ["a"], [1], 4), True),
        (([x + y <= 10, x + y >= 10], {"x": x, "y": y}, ["x", "y"], [1, 1], 99), False),  # NOT forced → None
    ]
    for (co, cs, nm, ints, k), want in cases:
        cert = motzkin_certificate(co, [str(z) for z in co], cs, nm, ints, k, False)
        if want and not cert:
            fails.append(f"#348 unit: expected a certificate for ints={ints} const={k}, got None")
        if not want and cert:
            fails.append(f"#348 anti-lie: an UNforced relation must yield NO certificate, got {cert!r}")


def _check_richer_basis(fails):
    """#350 — sympy's sparse .nullspace() returns integer-λ basis vectors only, so 2x=y ∧ 2z=y surfaces
    2·x=y/2·z=y but never the cleaner 3-var x+z=y (a fractional combo of the basis). The lattice
    enumeration must surface that minimal relation, Z3-verified."""
    with tempfile.TemporaryDirectory() as w:
        ok, prefix, *_ = _export("claim t\n    0 ≤ x ∈ Int ≤ 10\n    0 ≤ y ∈ Int ≤ 20\n    0 ≤ z ∈ Int ≤ 10\n"
                                 "    y = 2 * x\n    y = 2 * z", w)
        rels = solution_structure(prefix + ".smt2", prefix + ".schema.json").get("relations", [])
        eqs = [r["eq"] for r in rels]
        if "x + z = y" not in eqs:
            fails.append(f"#350: cleaner 3-var 'x + z = y' (fractional combo of the basis) not surfaced; got {eqs}")
        rec = next((r for r in rels if r["eq"] == "x + z = y"), None)
        if rec and not _reverify_smtlib(rec):   # the lattice candidate is genuinely forced, not a coincidence
            fails.append("#350: 'x + z = y' obligation should re-verify UNSAT in z3")
    # #350 must NOT flood: the #337 two-relation claim still surfaces EXACTLY its 2 minimal relations
    # (every larger-coefficient combination is dropped — only the cleanest generators survive).
    with tempfile.TemporaryDirectory() as w:
        ok, prefix, *_ = _export("claim t\n    0 ≤ a ∈ Int ≤ 5\n    0 ≤ b ∈ Int ≤ 5\n    0 ≤ c ∈ Int ≤ 10\n"
                                 "    0 ≤ d ∈ Int ≤ 15\n    c = a + b\n    d = a - b + 10", w)
        rels = solution_structure(prefix + ".smt2", prefix + ".schema.json").get("relations", [])
        if len(rels) != 2:
            fails.append(f"#350 no-flood: #337 claim must stay at 2 minimal relations, got {[r['eq'] for r in rels]}")


def main():
    fails = []
    for name, src, want_bb, want_free, want_eqs in CASES:
        with tempfile.TemporaryDirectory() as w:
            ok, prefix, dropped, msg = _export(src, w)
            if not ok:
                fails.append(f"{name}: export failed: {msg.splitlines()[0][:60]}")
                continue
            r = solution_structure(prefix + ".smt2", prefix + ".schema.json")
            if not r["sat"]:
                fails.append(f"{name}: unexpectedly UNSAT")
                continue
            bb = {_short(n) for n, _ in r["backbone"]}
            fr = {_short(n) for n, _ in r["free"]}
            eqs = {tuple(sorted((_short(a), _short(b)))) for a, b in r["equalities"]}
            want_eqs = {tuple(sorted(e)) for e in want_eqs}
            if bb != want_bb:
                fails.append(f"{name}: backbone {bb} != {want_bb}")
            if fr != want_free:
                fails.append(f"{name}: free {fr} != {want_free}")
            if eqs != want_eqs:
                fails.append(f"{name}: equalities {eqs} != {want_eqs}")

    # forced INEQUALITIES — a claim that forces two vars DIFFERENT (xor) must surface a≠b.
    with tempfile.TemporaryDirectory() as w:
        ok, prefix, *_ = _export("claim xor\n    a, b ∈ Bool\n    a ≠ b", w)
        r = solution_structure(prefix + ".smt2", prefix + ".schema.json")
        ineq = {tuple(sorted((_short(a), _short(b)))) for a, b in r.get("inequalities", [])}
        if ineq != {("a", "b")}:
            fails.append(f"xor: inequalities {ineq} != {{('a', 'b')}}")

    _check_relations(fails)
    _check_inequality_forced(fails)       # #348 — Farkas/Motzkin certificate for inequality-forced relations
    _check_richer_basis(fails)            # #350 — lattice surfaces minimal relations the sparse basis misses

    if fails:
        print("SOLUTION-STRUCTURE FAILURES:")
        for f in fails:
            print("  ✗", f)
        return 1
    print("✓ solution_structure: sys → backbone {a,b} + free c, coupled → forces x=y, xor → forces "
          "a≠b, packing → both free; #329/#337/#339 non-pairwise → a+b=c, a=b+3, ≥2 co-existing, "
          "real 2x=y; #341 each relation carries its forcing-constraint proof core; #348 inequality-"
          "forced relations carry the Farkas/Motzkin λ≥0 certificate (x+y=10 from ≤10 ∧ ≥10); #350 "
          "lattice surfaces the minimal x+z=y the sparse basis misses (no flood) — what a claim "
          "DETERMINES (Z3)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
