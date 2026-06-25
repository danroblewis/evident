#!/usr/bin/env python3
"""#376 — query / invariant may reference DERIVED (computed) vars, not just carried ones.

A derived var like `done = count≥5` is in every reachable state record (it's displayed), so the
verification interface must let you assert over it — the carried-only restriction leaked the FSM's
internal state encoding into the query/invariant API (Ana #376).

Run from repo root: python3 ide/test_derived_query.py
"""
import sys
import tempfile

sys.path.insert(0, "viz")
sys.path.insert(0, "ide/web")
from runtime_io import _export
from evident_viz import load as load_model

COUNTER = "fsm counter\n    count ∈ Int := 0\n    Δcount = (_count<5?1:0)\n    done ∈ Bool = (count≥5)"


def main():
    fails = []
    with tempfile.TemporaryDirectory() as w:
        ok, prefix, *_ = _export(COUNTER, w)
        m = load_model(prefix + ".smt2", prefix + ".schema.json")
        if "done" not in [v["name"] for v in m.derived]:
            fails.append("'done' should be a derived var")
        # invariant over a DERIVED var — and NON-vacuous: done flips true at count=5.
        inv = m.check_invariant("done", "=", "false", limit=400)
        if inv.get("holds") is not False:
            fails.append(f"invariant done=false should be VIOLATED (count=5 → done), holds={inv.get('holds')!r}")
        if (inv.get("counterexample") or {}).get("done") is not True:
            fails.append(f"counterexample should carry done=True, got {inv.get('counterexample')!r}")
        # query over a DERIVED var — finds the witness.
        q = m.query([("done", "=", "true")], limit=400)
        if (q.get("witness") or {}).get("done") is not True:
            fails.append(f"query ∃ done=true should find a witness with done=True, got {q.get('witness')!r}")
    if fails:
        print("✗ derived-var query/invariant (#376):")
        for f in fails:
            print("   -", f)
        sys.exit(1)
    print("✓ derived-var verification (#376): query/invariant resolve a DERIVED var (done = count≥5) and "
          "check it NON-vacuously — invariant done=false is violated at count=5 (ce carries done=True), "
          "query ∃ done=true finds the witness; the carried-only restriction is gone")


if __name__ == "__main__":
    main()
