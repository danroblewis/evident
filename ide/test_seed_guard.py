#!/usr/bin/env python3
"""#370 — a SEED guarded step (is_first_tick ⇒ x=0) is NOT 'INCOMPLETE'. The ¬first-tick case is the
var's separate difference equation, which the guarded step doesn't fold in. guard_analysis must label
it seed_only (complete once is_first_tick is assumed) rather than report a partial function (Ana #370).

Run from repo root: python3 ide/test_seed_guard.py
"""
import sys
import tempfile

sys.path.insert(0, "viz")
sys.path.insert(0, "ide/web")
from runtime_io import _export
from render_function_common import load_functions
from functionize import guard_analysis

RAMP = "fsm ramp\n    x ∈ Int := 0\n    Δx = (_x<5?2:(_x<12?1:0))"


def main():
    fails = []
    with tempfile.TemporaryDirectory() as w:
        ok, prefix, *_ = _export(RAMP, w)
        m, f = load_functions(prefix + ".smt2", prefix + ".schema.json")
        ga = guard_analysis(m, f["steps"], f["residual"])
        v = ga.get("x") or {}
        if v.get("complete") is not False:
            fails.append(f"seed step x is incomplete on its own, complete={v.get('complete')!r}")
        if v.get("seed_only") is not True:
            fails.append(f"seed step x (is_first_tick ⇒ x=0) should be seed_only=True, got {v.get('seed_only')!r}")
    if fails:
        print("✗ seed-guard verdict (#370):")
        for f_ in fails:
            print("   -", f_)
        sys.exit(1)
    print("✓ seed-guard verdict (#370): a Δ+seed var's guarded step (is_first_tick ⇒ x=0) is labelled "
          "seed_only (complete once is_first_tick is assumed — the ¬first-tick case is the Δ equation), "
          "not the misleading '⚠ INCOMPLETE'")


if __name__ == "__main__":
    main()
