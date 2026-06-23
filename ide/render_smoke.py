#!/usr/bin/env python3
"""Smoke-test the IDE's view renderers — the one thing ./test.sh's Rust tests + demos do NOT cover.

A refactor (or any viz/ change) that breaks a renderer is otherwise caught only by manually driving the
browser; this exercises the exact server render path headlessly. For a couple of scalar/enum samples it
exports the transition + renders EVERY registered view, asserting each produces a non-empty PNG with no
exception. Run from the repo root: `python3 ide/render_smoke.py` (exit non-zero on any failure).

Scoped to scalar/enum samples on purpose: effect-heavy FSMs round-trip a datatype through
z3.parse_smt2_file, which a newer z3 rejects ("repeated accessor f0") — a separate runtime/z3 concern,
not a render regression. These two samples load in the dev env's z3 and exercise both the dynamics and
the function views.
"""
import os
import sys
import tempfile

sys.path.insert(0, "ide/web")
sys.path.insert(0, "viz")

from runtime_io import _export                              # noqa: E402
from render import RENDERERS, VIEWS                         # noqa: E402

SAMPLES = {
    "traffic": (
        "enum Light = Red | Green | Yellow\n"
        "fsm traffic\n"
        "    light ∈ Light\n"
        "    0 ≤ timer ∈ Int ≤ 2\n"
        "    is_first_tick ⇒ (light = Red ∧ timer = 0)\n"
        "    (¬is_first_tick ∧ _timer < 2) ⇒ (light = _light ∧ timer = _timer + 1)\n"
        "    (¬is_first_tick ∧ _timer = 2 ∧ _light = Red) ⇒ (light = Green ∧ timer = 0)\n"
        "    (¬is_first_tick ∧ _timer = 2 ∧ _light = Green) ⇒ (light = Yellow ∧ timer = 0)\n"
        "    (¬is_first_tick ∧ _timer = 2 ∧ _light = Yellow) ⇒ (light = Red ∧ timer = 0)\n"),
    "counter": (
        "fsm counter\n"
        "    count ∈ Int\n"
        "    is_first_tick ⇒ count = 0\n"
        "    ¬is_first_tick ⇒ Δcount = (_count < 5 ? 1 : 0)\n"
        "    done ∈ Bool = (count ≥ 5)\n"),
}


def main():
    fails = []
    for name, src in SAMPLES.items():
        with tempfile.TemporaryDirectory() as work:
            ok, prefix, dropped, msg = _export(src, work)
            if not ok:
                fails.append(f"{name}: export failed: {msg.splitlines()[0][:60]}")
                continue
            for view in VIEWS:
                out = os.path.join(work, f"{view}.png")
                try:
                    RENDERERS[view](prefix + ".smt2", prefix + ".schema.json", out)
                    if not (os.path.exists(out) and os.path.getsize(out) > 0):
                        fails.append(f"{name}/{view}: produced no (or empty) PNG")
                except Exception as e:
                    fails.append(f"{name}/{view}: {type(e).__name__}: {e}")
    if fails:
        print("RENDER SMOKE-TEST FAILURES:")
        for f in fails:
            print("  ✗", f)
        return 1
    print(f"✓ render smoke-test: {len(SAMPLES)} samples × {len(VIEWS)} views all rendered a PNG")
    return 0


if __name__ == "__main__":
    sys.exit(main())
