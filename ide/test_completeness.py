#!/usr/bin/env python3
"""Test the BMC unroll's COMPLETENESS CERTIFICATION (Ana #270).

The k-step unroll export (`viz/model_analysis.py::unroll_smt2`) prepends a comment that turns a
BOUNDED check into a PROOF when the reachable set CLOSES within the unroll depth. This asserts the
right verdict for four model shapes:

  - counter (terminating)  → reachable set closes at a finite depth ≤ k → "COMPLETE at depth k=N"
  - traffic (cyclic)       → finite cyclic reachable set, closes within scope → "COMPLETE at depth k=N"
  - unbounded (Δ=1 ∀)      → grows past the scope cap → "BOUNDED" (never "COMPLETE")
  - real-valued (∈ Real)   → not exhaustively enumerable → "BOUNDED" (honesty gate, never "COMPLETE")

Also pins the CLOSING DEPTH for the discrete-closing models, and checks that `closing_depth()`
returns `complete=False` for the real and unbounded cases. Run from repo root:
`python3 ide/test_completeness.py` (exit non-zero on any failure)."""
import sys
import tempfile

sys.path.insert(0, "ide/web")
sys.path.insert(0, "viz")

from runtime_io import _export                              # noqa: E402
from evident_viz import load as load_model                  # noqa: E402

COUNTER = (
    "fsm counter\n"
    "    count ∈ Int\n"
    "    is_first_tick ⇒ count = 0\n"
    "    ¬is_first_tick ⇒ Δcount = (_count < 5 ? 1 : 0)\n")

TRAFFIC = (
    "enum Light = Red | Green | Yellow\n"
    "fsm traffic\n"
    "    light ∈ Light\n"
    "    0 ≤ timer ∈ Int ≤ 2\n"
    "    is_first_tick ⇒ (light = Red ∧ timer = 0)\n"
    "    (¬is_first_tick ∧ _timer < 2) ⇒ (light = _light ∧ timer = _timer + 1)\n"
    "    (¬is_first_tick ∧ _timer = 2 ∧ _light = Red) ⇒ (light = Green ∧ timer = 0)\n"
    "    (¬is_first_tick ∧ _timer = 2 ∧ _light = Green) ⇒ (light = Yellow ∧ timer = 0)\n"
    "    (¬is_first_tick ∧ _timer = 2 ∧ _light = Yellow) ⇒ (light = Red ∧ timer = 0)\n")

# No upper bound: count climbs forever, the reachable set never closes → capped → BOUNDED.
UNBOUNDED = (
    "fsm climber\n"
    "    count ∈ Int\n"
    "    is_first_tick ⇒ count = 0\n"
    "    ¬is_first_tick ⇒ Δcount = 1\n")

# Real-valued state: not a finite enumerable graph → never certified complete (Ana's honesty bar).
REAL = (
    "fsm decay\n"
    "    x ∈ Real\n"
    "    is_first_tick ⇒ x = 100\n"
    "    ¬is_first_tick ⇒ x = _x / 2\n")


def _load(src, work):
    ok, prefix, dropped, msg = _export(src, work)
    if not ok:
        raise RuntimeError(f"export failed: {msg.splitlines()[0][:80]}")
    return load_model(prefix + ".smt2", prefix + ".schema.json")


def check(name, src, *, expect_complete, expect_depth=None, k=8):
    with tempfile.TemporaryDirectory() as work:
        m = _load(src, work)
        smt = m.unroll_smt2(k)
        if smt is None:
            return [f"{name}: unroll_smt2 returned None (no transition)"]
        head = smt.splitlines()[0]
        d, complete = m.closing_depth()
        fails = []
        if expect_complete:
            if "; COMPLETE at depth k=" not in head:
                fails.append(f"{name}: expected COMPLETE wording, got: {head!r}")
            if not complete:
                fails.append(f"{name}: closing_depth complete=False, expected True")
            if expect_depth is not None and f"k={expect_depth}" not in head:
                fails.append(f"{name}: expected depth k={expect_depth}, got: {head!r}")
            if expect_depth is not None and d != expect_depth:
                fails.append(f"{name}: closing_depth d={d}, expected {expect_depth}")
        else:
            if "; BOUNDED" not in head:
                fails.append(f"{name}: expected BOUNDED wording, got: {head!r}")
            if complete:
                fails.append(f"{name}: closing_depth complete=True, expected False (honesty gate)")
        return fails


def main():
    fails = []
    # counter: count walks 0,1,2,3,4,5 then rests at 5 → 6 states, closes at depth 5.
    fails += check("counter", COUNTER, expect_complete=True, expect_depth=5)
    # traffic: finite cyclic graph, closes within scope (depth pinned by the round-trip length).
    fails += check("traffic", TRAFFIC, expect_complete=True)
    # unbounded: the reachable set never closes — hits the scope cap → BOUNDED, complete=False.
    fails += check("unbounded", UNBOUNDED, expect_complete=False)
    # real: not exhaustively enumerable → never COMPLETE regardless of whether the orbit settles.
    fails += check("real", REAL, expect_complete=False)
    if fails:
        print("COMPLETENESS-CERTIFICATION TEST FAILURES:")
        for f in fails:
            print("  ✗", f)
        return 1
    print("✓ completeness certification: COMPLETE for counter/traffic, BOUNDED for unbounded/real")
    return 0


if __name__ == "__main__":
    sys.exit(main())
