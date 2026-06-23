#!/usr/bin/env python3
"""Parameterized passthrough `..Name(field ↦ other)` parse + compose (#294).

Before this feature the parser ERRORED on `..Name(...)` ('expected schema/claim/...,
got ('). Now `..Name(slot ↦ other, …)` parses as a RENAME-arg list: the included
claim's renamed carried field follows the outer name, and every un-renamed carried
var (e.g. `da`) is freshened per-instance so two passthroughs of the SAME claim with
DIFFERENT renames compose into INDEPENDENT sub-systems.

Two halves, both pinned here:
  1. EXPORT (via the IDE's `_export`): the parameterized-passthrough fsm parses and
     analyzes with ZERO dropped constraints — for two-instance, single, and bare forms.
  2. RUNTIME (`evident test`): sat/unsat claims prove the COMPOSE semantics — both
     walks seed independently, each respects its own ±1 bound, and the two walks can
     step in opposite directions on the same tick (only possible if `da` is freshened
     per instance, not shared).

Run from repo root: `python3 ide/test_passthrough_params.py` (exit non-zero on failure)."""
import subprocess
import sys
import tempfile

sys.path.insert(0, "ide/web")

from config import EVIDENT, ROOT                            # noqa: E402
from runtime_io import _export                              # noqa: E402

ONE_D = (
    "fsm one_d_random_walk\n"
    "    a, da ∈ Int\n"
    "    -1 ≤ da ≤ 1\n"
    "    is_first_tick ⇒ a = 0\n"
    "    ¬is_first_tick ⇒ Δa = da\n\n")

# Two-instance compose: x and y are independent ±1 walks built from one 1D walk.
TWO_INSTANCE = ONE_D + (
    "fsm random_walk\n"
    "    x, y ∈ Int\n"
    "    ..one_d_random_walk(a ↦ x)\n"
    "    ..one_d_random_walk(a ↦ y)\n")

# Single rename still works.
SINGLE = ONE_D + (
    "fsm single_walk\n"
    "    x ∈ Int\n"
    "    ..one_d_random_walk(a ↦ x)\n")

# Bare passthrough (no args) unchanged.
BARE = ONE_D + (
    "fsm bare_walk\n"
    "    a ∈ Int\n"
    "    da ∈ Int\n"
    "    ..one_d_random_walk\n")

EXPORT_CASES = [
    ("two-instance compose (a↦x, a↦y)", TWO_INSTANCE),
    ("single rename (a↦x)",            SINGLE),
    ("bare passthrough unchanged",      BARE),
]

# Runtime sat/unsat claims pinning the COMPOSE semantics.
RUNTIME_FIXTURE = ONE_D + (
    "claim sat_seed\n"
    "    x ∈ Int\n"
    "    y ∈ Int\n"
    "    ..one_d_random_walk(a ↦ x)\n"
    "    ..one_d_random_walk(a ↦ y)\n"
    "    is_first_tick\n"
    "    x = 0\n"
    "    y = 0\n\n"
    "claim sat_independent_steps\n"
    "    x ∈ Int\n"
    "    y ∈ Int\n"
    "    ..one_d_random_walk(a ↦ x)\n"
    "    ..one_d_random_walk(a ↦ y)\n"
    "    ¬is_first_tick\n"
    "    _x = 5\n"
    "    _y = 5\n"
    "    x = 6\n"
    "    y = 4\n\n"
    "claim unsat_step_too_big\n"
    "    x ∈ Int\n"
    "    y ∈ Int\n"
    "    ..one_d_random_walk(a ↦ x)\n"
    "    ..one_d_random_walk(a ↦ y)\n"
    "    ¬is_first_tick\n"
    "    _x = 0\n"
    "    x = 2\n\n"
    "claim sat_single\n"
    "    x ∈ Int\n"
    "    ..one_d_random_walk(a ↦ x)\n"
    "    is_first_tick\n"
    "    x = 0\n\n"
    "claim sat_bare\n"
    "    a ∈ Int\n"
    "    da ∈ Int\n"
    "    ..one_d_random_walk\n"
    "    is_first_tick\n"
    "    a = 0\n")


def check_export():
    fails = []
    for label, src in EXPORT_CASES:
        with tempfile.TemporaryDirectory() as work:
            ok, _prefix, dropped, msg = _export(src, work, None)
            if not ok:
                fails.append(f"{label}: export FAILED — {msg.splitlines()[0][:90] if msg else '?'}")
            elif dropped != 0:
                fails.append(f"{label}: {dropped} dropped constraint(s) — compose lost a constraint")
    return fails


def check_runtime():
    """Run `evident test` on the sat/unsat fixture; all claims must pass, 0 dropped."""
    with tempfile.NamedTemporaryFile("w", suffix=".ev", delete=False) as f:
        f.write(RUNTIME_FIXTURE)
        path = f.name
    r = subprocess.run([EVIDENT, "test", path], capture_output=True, text=True,
                       timeout=60, cwd=ROOT)
    out = (r.stdout or "") + (r.stderr or "")
    fails = []
    dropped = sum(1 for ln in out.splitlines() if "dropped" in ln.lower())
    if dropped:
        fails.append(f"runtime: {dropped} dropped-constraint warning(s) — see output")
    # The test runner prints "N passed"; require 5 passing and no failures.
    if "5 passed" not in out or " failed" in out.lower().replace("0 failed", ""):
        fails.append(f"runtime: expected '5 passed' with no failures, got:\n{out.strip()[-400:]}")
    return fails


def main():
    fails = check_export() + check_runtime()
    if fails:
        print("FAIL: parameterized passthrough")
        for f in fails:
            print("  -", f)
        sys.exit(1)
    print("ok: parameterized passthrough — 3 export forms (0 dropped) + 5 runtime claims pass")


if __name__ == "__main__":
    main()
