"""
CLI interface conformance tests.

Tests the evident command-line interface: subcommands, flags, exit codes,
output format. Black-box — only tests what a user sees, not internals.

The surviving subcommands are `sample`, `test`, and `effect-run`. The
single SAT/UNSAT decision the former `query` made is now
`sample <file> <schema> -n 1`; the former `check` is `sample --all`.
"""

import json
from pathlib import Path
from .conftest import _evident, query, check, assert_sat

PROJECT_ROOT = Path(__file__).parent.parent.parent


# ---------------------------------------------------------------------------
# evident sample — single-model SAT/UNSAT decision (subsumes `query`)
# ---------------------------------------------------------------------------

def test_sample_sat_exit_code(tmp_path):
    f = tmp_path / 'test.ev'
    f.write_text("schema S\n    x ∈ Nat\n    x = 5\n")
    r = _evident('sample', str(f), 'S', '-n', '1')
    assert r.returncode == 0

def test_sample_json_sat_returns_bindings(tmp_path):
    """SAT: `sample --json` returns a one-element array of bindings."""
    f = tmp_path / 'test.ev'
    f.write_text("schema S\n    x ∈ Nat\n    x = 42\n")
    r = _evident('sample', str(f), 'S', '-n', '1', '--json')
    assert r.returncode == 0
    assert json.loads(r.stdout) == [{'x': 42}]

def test_sample_json_unsat_returns_empty(tmp_path):
    """UNSAT: `sample --json` returns an empty array."""
    f = tmp_path / 'test.ev'
    f.write_text("schema S\n    x ∈ Nat\n    x < 0\n")
    r = _evident('sample', str(f), 'S', '-n', '1', '--json')
    assert json.loads(r.stdout) == []

def test_sample_unknown_schema(tmp_path):
    f = tmp_path / 'test.ev'
    f.write_text("schema S\n    x ∈ Nat\n")
    r = _evident('sample', str(f), 'DoesNotExist', '-n', '1')
    assert r.returncode != 0

def test_query_helper_rides_sample_with_given():
    # The repointed query() helper rides `sample`; --given still works.
    src = "schema S\n    x ∈ Nat\n    y ∈ Nat\n    x + y = 10\n"
    b = assert_sat(query(src, 'S', {'x': 3}))
    assert b['x'] == 3
    assert b['y'] == 7


# ---------------------------------------------------------------------------
# evident sample --all — batch sat-check (subsumes `check`)
# ---------------------------------------------------------------------------

def test_sample_all_reports_sat_and_unsat():
    src = "schema A\n    x ∈ Nat\n\nschema B\n    x ∈ Nat\n    x < 0\n"
    results = check(src)
    assert results['A'] is True
    assert results['B'] is False


# ---------------------------------------------------------------------------
# Removed subcommands are unrecognised
# ---------------------------------------------------------------------------

def test_removed_subcommands_are_unrecognised():
    """Only sample/test/effect-run survive; the rest exit non-zero."""
    for sub in ('run', 'query', 'check', 'lint', 'profile', 'desugar', 'infer-types'):
        r = _evident(sub, '/dev/null')
        assert r.returncode != 0, f"`evident {sub}` should be unrecognised"


# ---------------------------------------------------------------------------
# Parse errors produce non-zero exit
# ---------------------------------------------------------------------------

def test_parse_error_exit_code(tmp_path):
    f = tmp_path / 'bad.ev'
    f.write_text("schema S\n    x ∈\n")  # incomplete membership
    r = _evident('sample', str(f), '--all')
    assert r.returncode != 0

def test_parse_error_message(tmp_path):
    f = tmp_path / 'bad.ev'
    f.write_text("this is not valid evident syntax !!!\n")
    r = _evident('sample', str(f), '--all')
    assert r.returncode != 0
    # Should print something to stderr or stdout indicating an error
    assert r.stderr or r.stdout


# ---------------------------------------------------------------------------
# Import resolution
# ---------------------------------------------------------------------------

def test_import_resolves(tmp_path):
    lib = tmp_path / 'lib.ev'
    lib.write_text("schema Point\n    x ∈ Int\n    y ∈ Int\n")
    main = tmp_path / 'main.ev'
    main.write_text(f'import "{lib}"\n\nschema S\n    p ∈ Point\n    p.x = 3\n    p.y = 4\n')
    r = _evident('sample', str(main), 'S', '-n', '1', '--json')
    assert r.returncode == 0
    data = json.loads(r.stdout)
    assert data and data[0]['p.x'] == 3


# ---------------------------------------------------------------------------
# Multiple schemas in one file
# ---------------------------------------------------------------------------

def test_multiple_schemas(tmp_path):
    f = tmp_path / 'multi.ev'
    f.write_text("schema A\n    x ∈ Nat\n    x = 1\n\nschema B\n    x ∈ Nat\n    x = 2\n")
    ra = _evident('sample', str(f), 'A', '-n', '1', '--json')
    rb = _evident('sample', str(f), 'B', '-n', '1', '--json')
    assert ra.returncode == 0 and rb.returncode == 0
    assert json.loads(ra.stdout)[0]['x'] == 1
    assert json.loads(rb.stdout)[0]['x'] == 2
