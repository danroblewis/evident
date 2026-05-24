"""
Conformance tests for `evident profile <files…> <schema> [--given …] [--top N]`.

Black-box: writes a small claim, runs the CLI, and asserts on the
human-readable report — the given/solved-for var lists plus the ranked
bottleneck table.
"""

from .conftest import _evident

ADDER = """claim Adder(x ∈ Int, y ∈ Int)
    sum ∈ Int
    diff ∈ Int
    sum = x + y
    diff = x - y
"""

CONTRADICTION = """claim Contradiction(x ∈ Int)
    y ∈ Int
    y = x + 1
    y = x + 2
"""


def test_profile_reports_given_solved_and_bottleneck(tmp_path):
    f = tmp_path / 'adder.ev'
    f.write_text(ADDER)
    r = _evident('profile', str(f), 'Adder')
    assert r.returncode == 0, r.stderr
    out = r.stdout
    # Header + the two cheap AST-only sections.
    assert '== Claim "Adder" ==' in out
    assert 'Given (caller-supplied):' in out
    assert 'Solved for' in out
    # Params appear under Given; outputs under Solved for.
    assert 'x' in out and 'y' in out
    assert 'sum' in out and 'diff' in out
    # The bottleneck ranking with the documented columns.
    assert 'Bottleneck analysis' in out
    assert 'baseline(μs)' in out
    assert 'savings(μs)' in out
    # At least one ranked row (rank 1).
    assert any(line.strip().startswith('1') for line in out.splitlines())


def test_profile_top_limits_rows(tmp_path):
    f = tmp_path / 'adder.ev'
    f.write_text(ADDER)
    r = _evident('profile', str(f), 'Adder', '--top', '1')
    assert r.returncode == 0, r.stderr
    # Count ranked rows: lines whose first token is an integer.
    ranks = [ln for ln in r.stdout.splitlines()
             if ln.strip()[:1].isdigit() and ln.strip().split()[0].isdigit()]
    assert len(ranks) <= 1, f"--top 1 should cap rows, got: {ranks}"


def test_profile_unsat_baseline_errors(tmp_path):
    f = tmp_path / 'bad.ev'
    f.write_text(CONTRADICTION)
    r = _evident('profile', str(f), 'Contradiction')
    assert r.returncode != 0
    assert 'UNSAT' in (r.stdout + r.stderr)


def test_profile_unknown_schema_errors(tmp_path):
    f = tmp_path / 'adder.ev'
    f.write_text(ADDER)
    r = _evident('profile', str(f), 'Nope')
    assert r.returncode != 0
