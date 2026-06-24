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
        rel = [x["eq"] for x in solution_structure(prefix + ".smt2", prefix + ".schema.json").get("relations", [])]
        if "2·x = y" not in rel:
            fails.append(f"#339 real: '2·x = y' not in {rel}")
    # #341: each relation carries its UNSAT-core proof. A DERIVED relation (a+b=10 ∧ b+c=12 together force
    # c=a+2) must cite BOTH forcing constraints in its core — the interrogable "why is this forced".
    with tempfile.TemporaryDirectory() as w:
        ok, prefix, *_ = _export("claim t\n    0 ≤ a ∈ Int ≤ 10\n    0 ≤ b ∈ Int ≤ 10\n    0 ≤ c ∈ Int ≤ 12\n"
                                 "    a + b = 10\n    b + c = 12", w)
        rels = solution_structure(prefix + ".smt2", prefix + ".schema.json").get("relations", [])
        forced = [x for x in rels if any("a + b" in cc for cc in x["core"]) and any("b + c" in cc for cc in x["core"])]
        if not forced:
            fails.append(f"#341: no relation cites both forcing constraints; got {[(x['eq'], x['core']) for x in rels]}")


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

    if fails:
        print("SOLUTION-STRUCTURE FAILURES:")
        for f in fails:
            print("  ✗", f)
        return 1
    print("✓ solution_structure: sys → backbone {a,b} + free c, coupled → forces x=y, xor → forces "
          "a≠b, packing → both free; #329/#337/#339 non-pairwise → a+b=c, a=b+3, ≥2 co-existing, "
          "real 2x=y; #341 each relation carries its forcing-constraint proof core — what a claim "
          "DETERMINES (Z3)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
