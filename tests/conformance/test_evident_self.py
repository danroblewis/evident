"""
Conformance tests for the `evident-self` CLI wrapper (Phase D3).

The wrapper is a proof-of-concept stand-in for the self-hosted
compiler. Phase D2 will replace its body with a real Evident
pipeline driver; Phase E will sever the Rust bootstrap entirely.
These tests assert the CLI SURFACE the wrapper presents — which
stays stable across those phases.
"""

import subprocess
from pathlib import Path

PROJECT_ROOT = Path(__file__).parent.parent.parent
EVIDENT_SELF = PROJECT_ROOT / "scripts" / "evident-self"


def _run(*args: str, timeout: int = 30) -> subprocess.CompletedProcess:
    return subprocess.run(
        [str(EVIDENT_SELF), *args],
        capture_output=True,
        text=True,
        timeout=timeout,
        cwd=PROJECT_ROOT,
    )


def _trivial_source(tmp_path: Path) -> Path:
    f = tmp_path / "demo.ev"
    f.write_text("schema S\n    x ∈ Int\n    x = 1\n")
    return f


# ---------------------------------------------------------------------------
# Script presence + help surface
# ---------------------------------------------------------------------------

def test_script_exists_and_is_executable():
    assert EVIDENT_SELF.exists(), f"missing {EVIDENT_SELF}"
    import os
    assert os.access(EVIDENT_SELF, os.X_OK), f"{EVIDENT_SELF} not executable"


def test_help_mentions_emit_subcommand():
    r = _run("--help")
    assert r.returncode == 0
    assert "emit" in (r.stdout + r.stderr)


def test_no_args_prints_usage():
    r = _run()
    # Either exits 0 with usage (chose this) or non-zero with usage;
    # be permissive on the exit code, strict on the message.
    assert "Usage" in (r.stdout + r.stderr) or "usage" in (r.stdout + r.stderr).lower()


# ---------------------------------------------------------------------------
# Happy path: emit produces SMT-LIB output
# ---------------------------------------------------------------------------

def test_emit_writes_smtlib_to_output_file(tmp_path):
    src = _trivial_source(tmp_path)
    out = tmp_path / "out.smt2"
    r = _run("emit", str(src), "S", "-o", str(out))
    assert r.returncode == 0, f"stderr: {r.stderr}"
    assert out.exists(), f"-o file not created: {out}"
    text = out.read_text()
    # The pipeline emits a minimal SMT-LIB fragment. Phase D3
    # contract: the output is a valid-looking SMT-LIB program.
    assert "(declare-fun" in text, f"no declare-fun in output:\n{text}"
    assert "(assert" in text,      f"no assert in output:\n{text}"
    assert "(check-sat)" in text,  f"no check-sat in output:\n{text}"


def test_emit_to_stdout_when_no_output_flag(tmp_path):
    src = _trivial_source(tmp_path)
    r = _run("emit", str(src), "S")
    assert r.returncode == 0, f"stderr: {r.stderr}"
    assert "(declare-fun" in r.stdout
    assert "(check-sat)" in r.stdout


# ---------------------------------------------------------------------------
# Error surface
# ---------------------------------------------------------------------------

def test_unknown_subcommand_is_rejected():
    r = _run("compile", "/dev/null")
    assert r.returncode != 0
    assert "unknown" in (r.stderr + r.stdout).lower() or \
           "expected" in (r.stderr + r.stdout).lower()


def test_emit_requires_input_file():
    r = _run("emit")
    assert r.returncode != 0
    assert "missing" in (r.stderr + r.stdout).lower() or \
           "file" in (r.stderr + r.stdout).lower()


def test_emit_requires_claim(tmp_path):
    src = _trivial_source(tmp_path)
    r = _run("emit", str(src))
    assert r.returncode != 0


def test_emit_rejects_missing_input_file(tmp_path):
    missing = tmp_path / "nope.ev"
    r = _run("emit", str(missing), "S")
    assert r.returncode != 0
    assert "not found" in (r.stderr + r.stdout).lower() or \
           "no such" in (r.stderr + r.stdout).lower()


def test_emit_rejects_unknown_flag(tmp_path):
    src = _trivial_source(tmp_path)
    r = _run("emit", str(src), "S", "--bogus")
    assert r.returncode != 0
