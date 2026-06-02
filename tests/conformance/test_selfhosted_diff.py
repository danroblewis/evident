"""
Conformance test for the Phase E2 diff-test harness.

E2 (per docs/plans/completion-roadmap.md) is "byte-for-byte match
between Rust and self-hosted output on a corpus." This test exercises
the harness skeleton: it asserts that
`scripts/diff-test-selfhosted.sh` runs the three Phase-D pipeline
fixtures (lex+parse+translate compositions) end-to-end and exits 0
with the expected one-line summary.

The harness body grows as the self-hosted compiler does — but the
CLI contract this test asserts (exit 0, fixture-count line) stays
stable across phases.
"""

import subprocess
from pathlib import Path

PROJECT_ROOT = Path(__file__).parent.parent.parent
HARNESS = PROJECT_ROOT / "scripts" / "diff-test-selfhosted.sh"


def _run(timeout: int = 120) -> subprocess.CompletedProcess:
    return subprocess.run(
        [str(HARNESS)],
        capture_output=True,
        text=True,
        timeout=timeout,
        cwd=PROJECT_ROOT,
    )


def test_harness_script_exists_and_is_executable():
    import os
    assert HARNESS.exists(), f"missing {HARNESS}"
    assert os.access(HARNESS, os.X_OK), f"{HARNESS} not executable"


def test_harness_exits_zero_on_clean_run():
    r = _run()
    assert r.returncode == 0, (
        f"diff-test-selfhosted.sh failed:\n"
        f"  stdout: {r.stdout}\n"
        f"  stderr: {r.stderr}"
    )


def test_harness_emits_summary_line():
    r = _run()
    out = r.stdout + r.stderr
    # The summary line is the load-bearing contract of E2's skeleton.
    # Full E2 (byte-for-byte corpus replication) extends this format.
    assert "selfhosted-pipeline:" in out, (
        f"missing 'selfhosted-pipeline:' summary line in:\n{out}"
    )
    assert "3/3" in out, (
        f"expected '3/3' fixtures-passed count in summary; got:\n{out}"
    )
    assert "lex+parse+translate" in out, (
        f"summary line does not describe the pipeline stages:\n{out}"
    )


def test_harness_reports_each_fixture():
    r = _run()
    out = r.stdout + r.stderr
    # Per-fixture status lines let CI surface which fixture broke
    # without having to re-run the script.
    for fixture in (
        "test_pipeline_full.ev",
        "test_pipeline_full_d2.ev",
        "test_pipeline_lex_parse.ev",
    ):
        assert fixture in out, f"fixture {fixture} not reported in:\n{out}"
