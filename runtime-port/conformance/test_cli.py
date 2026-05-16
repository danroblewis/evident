"""
CLI interface conformance tests.

Tests the evident command-line interface: subcommands, flags, exit codes,
output format. Black-box — only tests what a user sees, not internals.
"""

import json
import pytest
from pathlib import Path
from .conftest import _evident, query, assert_sat, assert_unsat

PROJECT_ROOT = Path(__file__).parent.parent.parent

SIMPLE_SCHEMA = """
schema Box
    width  ∈ Nat
    height ∈ Nat
    width < 100
    height < 100
    width > height
"""


# ---------------------------------------------------------------------------
# evident query
# ---------------------------------------------------------------------------

def test_query_sat_exit_code(tmp_path):
    f = tmp_path / 'test.ev'
    f.write_text("schema S\n    x ∈ Nat\n    x = 5\n")
    r = _evident('query', str(f), 'S')
    assert r.returncode == 0

def test_query_unsat_exit_code(tmp_path):
    f = tmp_path / 'test.ev'
    f.write_text("schema S\n    x ∈ Nat\n    x < 0\n")
    r = _evident('query', str(f), 'S')
    assert r.returncode != 0

def test_query_json_sat_returns_bindings(tmp_path):
    """SAT: CLI returns {"satisfied": true, "bindings": {...}}."""
    f = tmp_path / 'test.ev'
    f.write_text("schema S\n    x ∈ Nat\n    x = 42\n")
    r = _evident('query', str(f), 'S', '--json')
    assert r.returncode == 0
    data = json.loads(r.stdout)
    assert data['satisfied'] is True
    assert data['bindings']['x'] == 42

def test_query_json_unsat_returns_satisfied_false(tmp_path):
    """UNSAT: CLI returns {"satisfied": false} with non-zero exit."""
    f = tmp_path / 'test.ev'
    f.write_text("schema S\n    x ∈ Nat\n    x < 0\n")
    r = _evident('query', str(f), 'S', '--json')
    assert r.returncode != 0
    data = json.loads(r.stdout)
    assert data['satisfied'] is False

def test_query_with_given(tmp_path):
    f = tmp_path / 'test.ev'
    f.write_text("schema S\n    x ∈ Nat\n    y ∈ Nat\n    x + y = 10\n")
    r = _evident('query', str(f), 'S', '--json', '--given', 'x=3')
    assert r.returncode == 0
    data = json.loads(r.stdout)
    assert data['satisfied'] is True
    assert data['bindings']['x'] == 3
    assert data['bindings']['y'] == 7

def test_query_unknown_schema(tmp_path):
    f = tmp_path / 'test.ev'
    f.write_text("schema S\n    x ∈ Nat\n")
    r = _evident('query', str(f), 'DoesNotExist', '--json')
    assert r.returncode != 0


# ---------------------------------------------------------------------------
# evident check
# ---------------------------------------------------------------------------

def test_check_reports_sat_and_unsat(tmp_path):
    f = tmp_path / 'test.ev'
    f.write_text("schema A\n    x ∈ Nat\n\nschema B\n    x ∈ Nat\n    x < 0\n")
    r = _evident('check', str(f))
    output = r.stdout + r.stderr
    assert 'A' in output
    assert 'B' in output


# ---------------------------------------------------------------------------
# evident run was removed — ? queries belong in the REPL, not as a subcommand
# ---------------------------------------------------------------------------

def test_run_is_removed():
    """evident run no longer exists; the REPL handles interactive ? queries."""
    r = _evident('run', '/dev/null')
    assert r.returncode != 0   # unrecognised subcommand


# ---------------------------------------------------------------------------
# Parse errors produce non-zero exit
# ---------------------------------------------------------------------------

def test_parse_error_exit_code(tmp_path):
    f = tmp_path / 'bad.ev'
    f.write_text("schema S\n    x ∈\n")  # incomplete membership
    r = _evident('check', str(f))
    assert r.returncode != 0

def test_parse_error_message(tmp_path):
    f = tmp_path / 'bad.ev'
    f.write_text("this is not valid evident syntax !!!\n")
    r = _evident('check', str(f))
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
    r = _evident('query', str(main), 'S', '--json')
    assert r.returncode == 0
    data = json.loads(r.stdout)
    assert data['satisfied'] is True
    assert data['bindings']['p.x'] == 3


# ---------------------------------------------------------------------------
# Multiple schemas in one file
# ---------------------------------------------------------------------------

def test_multiple_schemas(tmp_path):
    f = tmp_path / 'multi.ev'
    f.write_text("schema A\n    x ∈ Nat\n    x = 1\n\nschema B\n    x ∈ Nat\n    x = 2\n")
    ra = _evident('query', str(f), 'A', '--json')
    rb = _evident('query', str(f), 'B', '--json')
    assert ra.returncode == 0 and rb.returncode == 0
    assert json.loads(ra.stdout)['bindings']['x'] == 1
    assert json.loads(rb.stdout)['bindings']['x'] == 2
