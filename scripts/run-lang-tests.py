#!/usr/bin/env python3
"""Drive every tests/lang_tests/*.ev through `evident sample --all --json`.

Asserts that claims prefixed `sat_` are SAT and `unsat_` are UNSAT. Returns
non-zero on the first mismatch. The single source of truth for language
correctness; conformance/ tests the CLI, these test the language itself."""

import subprocess
import sys
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BIN = ROOT / "bootstrap/runtime/target/release/evident"
LANG_TESTS = ROOT / "tests/lang_tests"


def main() -> int:
    if not BIN.exists():
        print(f"binary missing at {BIN}; run cargo build --release first", file=sys.stderr)
        return 1
    failed = []
    total = 0
    files = sorted(LANG_TESTS.glob("*.ev"))
    for f in files:
        r = subprocess.run(
            [str(BIN), "sample", str(f), "--all", "--json"],
            capture_output=True, text=True, timeout=60,
        )
        if r.returncode != 0:
            failed.append((f.name, "load", r.stderr.strip()[:300]))
            continue
        try:
            results = json.loads(r.stdout)
        except json.JSONDecodeError:
            failed.append((f.name, "json", r.stdout[:300]))
            continue
        for name, sat in results.items():
            total += 1
            if name.startswith("sat_") and not sat:
                failed.append((f.name, name, "expected sat, got unsat"))
            elif name.startswith("unsat_") and sat:
                failed.append((f.name, name, "expected unsat, got sat"))
    print(f"{len(files)} files, {total} claims, {len(failed)} failed")
    for f, n, m in failed:
        print(f"  FAIL {f}::{n}: {m}")
    return 0 if not failed else 1


if __name__ == "__main__":
    sys.exit(main())
