#!/usr/bin/env python3
"""Test: scatter_matrix samples a CLAIM/Solve program's SOLUTION SPACE (diagram review #5).

Before this, the sampling views (scatter_matrix and friends) only handled FSM carried-state:
they read `schema["fsm"]` and BFS-walked `reachable()`, so a pure `claim`/Solve program (free
decision variables, NO transition) either KeyError'd or rendered empty. The fix gave
`render_scatter_matrix` a claim path: detect the no-fsm schema, ENUMERATE distinct satisfying
assignments (block-and-resolve, the same witness enumeration the solve panel uses), and render
the pairwise scatter matrix over the claim's numeric variables — exactly the FSM picture, but
sampling SOLUTIONS instead of states.

`render_scatter_matrix.claim_witnesses(smt2, schema)` is the test seam: it returns
(states, plot_vars, cat_vars, enum_variants, feasible) without drawing. A real solution cloud is
`feasible=True` with ≥2 numeric plot_vars and many distinct witnesses; we pin BOTH the count AND
the constraint (every witness must actually satisfy x≥y ∧ x+y≤N — never a fabricated cloud). The
honest empties are pinned too: an UNSAT claim yields no witnesses; a categorical-only claim (enum,
no numeric) yields no plot_vars (→ the renderer's "see claim_space" card, not a scatter).

Run from repo root: `python3 ide/test_scatter_claim.py` (exit non-zero on any failure)."""
import os
import sys
import tempfile

sys.path.insert(0, "ide/web")
sys.path.insert(0, "viz")

from runtime_io import _export                       # noqa: E402
import render_scatter_matrix as RSM                  # noqa: E402

# A claim with TWO bounded Int decision variables + a constraint carving out a triangular
# feasible region. The solution space is the set of integer (x, y) with 0≤x,y≤20, x≥y, x+y≤20 —
# a real 2-D cloud, exactly what the scatter matrix should show.
NUMERIC = (
    "claim box\n"
    "    0 ≤ x ∈ Int ≤ 20\n"
    "    0 ≤ y ∈ Int ≤ 20\n"
    "    x ≥ y\n"
    "    x + y ≤ 20\n"
)

# UNSAT: no assignment satisfies it → no witnesses → the honest empty card (not a crash/empty plot).
UNSAT = (
    "claim impossible\n"
    "    0 ≤ x ∈ Int ≤ 5\n"
    "    x > 10\n"
)

# CATEGORICAL-only: enum vars, no numeric axis. The scatter matrix has nothing to put on an axis,
# so plot_vars is empty (the renderer then shows the "solution space is categorical" card).
ENUM_ONLY = (
    "enum Hue = Red | Green | Blue\n"
    "claim coloring\n"
    "    a ∈ Hue\n"
    "    b ∈ Hue\n"
    "    a ≠ b\n"
)


def _witnesses(src, work):
    ok, prefix, _dropped, msg = _export(src, work)
    if not ok:
        return None, f"export failed: {(msg or '').splitlines()[0][:80]}"
    smt2, schema = prefix + ".smt2", prefix + ".schema.json"
    return (RSM.claim_witnesses(smt2, schema), (smt2, schema))


def _render(info, work, fname):
    """Run the renderer headlessly and return its output PNG path."""
    smt2, schema = info
    out = os.path.join(work, fname)
    sys.argv = ["x", smt2, schema, out]
    RSM.main()
    return out


def _short(s, nm):
    return next(s[k] for k in s if k.split(".")[-1] == nm)


def _check_numeric(work):
    """A REAL solution cloud — ≥2 numeric vars, many DISTINCT witnesses, EVERY one satisfying the
    constraint (never a fabricated point), plus a non-trivial rendered PNG."""
    res, info = _witnesses(NUMERIC, work)
    if res is None:
        return [f"numeric: {info}"]
    states, plot_vars, _cat, _ev, feasible = res
    names = sorted(v["name"].split(".")[-1] for v in plot_vars)
    if not feasible:
        return ["numeric: claim is satisfiable but feasible=False"]
    if names != ["x", "y"]:
        return [f"numeric: expected plot_vars [x, y], got {names}"]
    if len(states) < 10:
        return [f"numeric: expected a dense solution cloud, got {len(states)} witness(es)"]
    bad = [s for s in states
           if not (0 <= _short(s, "x") <= 20 and 0 <= _short(s, "y") <= 20
                   and _short(s, "x") >= _short(s, "y") and _short(s, "x") + _short(s, "y") <= 20)]
    if bad:
        return [f"numeric: {len(bad)} witness(es) violate the claim — "
                f"e.g. {({k.split('.')[-1]: v for k, v in bad[0].items()})}"]
    if len({(_short(s, "x"), _short(s, "y")) for s in states}) != len(states):
        return ["numeric: witnesses are not distinct — block-and-resolve failed"]
    out = _render(info, work, "scatter.png")
    if not (os.path.exists(out) and os.path.getsize(out) > 5000):
        return ["numeric: renderer produced no / trivial PNG"]
    return []


def _check_unsat(work):
    """UNSAT claim: no witnesses, but the renderer still produces the honest empty card (no crash)."""
    res, info = _witnesses(UNSAT, work)
    if res is None:
        return [f"unsat: {info}"]
    states, _plot, _cat, _ev, feasible = res
    if feasible or states:
        return [f"unsat: expected feasible=False / 0 witnesses, got {feasible} / {len(states)}"]
    out = _render(info, work, "unsat.png")
    if not (os.path.exists(out) and os.path.getsize(out) > 1000):
        return ["unsat: renderer did not produce the empty-state card"]
    return []


def _check_enum_only(work):
    """CATEGORICAL-only claim: no numeric var → 0 plot_vars (→ the 'see claim_space' empty card)."""
    res, info = _witnesses(ENUM_ONLY, work)
    if res is None:
        return [f"enum-only: {info}"]
    _states, plot_vars, _cat, _ev, feasible = res
    if not feasible:
        return ["enum-only: 3-coloring is satisfiable but feasible=False"]
    if plot_vars:
        return [f"enum-only: a no-numeric claim must have 0 plot_vars, got "
                f"{[v['name'] for v in plot_vars]}"]
    out = _render(info, work, "enum.png")
    if not (os.path.exists(out) and os.path.getsize(out) > 1000):
        return ["enum-only: renderer did not produce the categorical empty-state card"]
    return []


def main():
    fails = []
    for check in (_check_numeric, _check_unsat, _check_enum_only):
        with tempfile.TemporaryDirectory() as work:
            fails += check(work)

    if fails:
        print("SCATTER-MATRIX CLAIM TEST FAILURES:")
        for f in fails:
            print("  ✗", f)
        return 1
    print("✓ scatter_matrix claim/Solve: a 2-Int claim renders its SOLUTION SPACE as a real cloud "
          "of distinct, constraint-satisfying z3 witnesses; an UNSAT claim and a categorical-only "
          "claim each fall to the honest empty-state card (no crash, no fabricated cloud)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
