"""
Phase E3 conformance: self-hosted pipeline perf gate.

The benchmark script (`scripts/bench-selfhosted.sh`) times the three
kernel pipeline fixtures end-to-end and exits 0 when total wall-clock
is under the threshold (60s per docs/plans/completion-roadmap.md, the
E3 acceptance criterion).

These tests assert the gate holds today: pipeline compile+run stays
well under the budget. If a future regression makes self-hosted
compiles slow, this test goes red — that's the signal.
"""

import subprocess
from pathlib import Path

PROJECT_ROOT = Path(__file__).parent.parent.parent
BENCH = PROJECT_ROOT / "scripts" / "bench-selfhosted.sh"
EVIDENT = PROJECT_ROOT / "runtime" / "target" / "release" / "evident"
KERNEL = PROJECT_ROOT / "kernel" / "target" / "release" / "kernel"


def test_bench_script_exists_and_is_executable():
    assert BENCH.exists(), f"missing {BENCH}"
    import os
    assert os.access(BENCH, os.X_OK), f"{BENCH} not executable"


def test_bench_help_describes_threshold():
    r = subprocess.run(
        [str(BENCH), "--help"],
        capture_output=True, text=True, timeout=10, cwd=PROJECT_ROOT,
    )
    assert r.returncode == 0
    text = r.stdout + r.stderr
    # Help should mention the < 60s gate.
    assert "60s" in text or "60 s" in text, f"help missing threshold:\n{text}"


def test_bench_passes_under_threshold():
    """Run the bench; assert exit 0 (total wall-clock < 60s)."""
    if not EVIDENT.exists() or not KERNEL.exists():
        import pytest
        pytest.skip("evident or kernel binary missing — build first via test.sh")

    r = subprocess.run(
        [str(BENCH)],
        capture_output=True, text=True, timeout=180, cwd=PROJECT_ROOT,
    )
    # Exit 0 = under threshold; 1 = over; 2 = a fixture failed.
    assert r.returncode == 0, (
        f"bench-selfhosted.sh exited {r.returncode}\n"
        f"stdout:\n{r.stdout}\n"
        f"stderr:\n{r.stderr}"
    )
    # Sanity: the report should mention every fixture by name.
    for name in (
        "test_pipeline_lex_parse.ev",
        "test_pipeline_full.ev",
        "test_pipeline_full_d2.ev",
    ):
        assert name in r.stdout, f"bench output missing {name}:\n{r.stdout}"
    assert "PASS" in r.stdout, f"bench output missing PASS marker:\n{r.stdout}"


def test_bench_quiet_mode_suppresses_banner():
    if not EVIDENT.exists() or not KERNEL.exists():
        import pytest
        pytest.skip("evident or kernel binary missing — build first via test.sh")

    r = subprocess.run(
        [str(BENCH), "--quiet"],
        capture_output=True, text=True, timeout=180, cwd=PROJECT_ROOT,
    )
    assert r.returncode == 0
    # No banner / per-fixture lines / total line in quiet mode.
    assert "threshold" not in r.stdout
    assert "total wall" not in r.stdout


def test_bench_rejects_unknown_flag():
    r = subprocess.run(
        [str(BENCH), "--bogus"],
        capture_output=True, text=True, timeout=10, cwd=PROJECT_ROOT,
    )
    assert r.returncode != 0
