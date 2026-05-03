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
    """SAT: CLI returns the bindings dict directly (no 'satisfied' wrapper)."""
    f = tmp_path / 'test.ev'
    f.write_text("schema S\n    x ∈ Nat\n    x = 42\n")
    r = _evident('query', str(f), 'S', '--json')
    assert r.returncode == 0
    data = json.loads(r.stdout)
    assert data['x'] == 42

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
    assert data['x'] == 3
    assert data['y'] == 7

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
# evident run (execute ? queries in file)
# ---------------------------------------------------------------------------

def test_run_query_statement(tmp_path):
    """evident run executes ? queries in the file."""
    f = tmp_path / 'test.ev'
    # Use a membership query which is supported
    f.write_text("schema S\n    x ∈ Nat\n    x = 7\n? S\n")
    r = _evident('run', str(f))
    assert r.returncode == 0


# ---------------------------------------------------------------------------
# evident execute (automaton streaming)
# ---------------------------------------------------------------------------

def test_execute_ev_nl(tmp_path):
    prog = PROJECT_ROOT / 'programs' / 'ev-nl.ev'
    r = _evident('execute', str(prog), stdin="hello\nworld\n")
    assert r.returncode == 0
    assert r.stdout == "1\thello\n2\tworld\n"

def test_execute_empty_input(tmp_path):
    prog = PROJECT_ROOT / 'programs' / 'ev-nl.ev'
    r = _evident('execute', str(prog), stdin="")
    assert r.returncode == 0
    assert r.stdout == ""

def test_execute_batch_nl():
    prog = PROJECT_ROOT / 'programs' / 'nl-batch.ev'
    r = _evident('execute', str(prog), stdin="a\nb\nc\n")
    assert r.returncode == 0
    assert r.stdout == "1\ta\n2\tb\n3\tc\n"


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
    data = json.loads(r.stdout)   # SAT: raw bindings dict
    assert data['p.x'] == 3


# ---------------------------------------------------------------------------
# Multiple schemas in one file
# ---------------------------------------------------------------------------

def test_multiple_schemas(tmp_path):
    f = tmp_path / 'multi.ev'
    f.write_text("schema A\n    x ∈ Nat\n    x = 1\n\nschema B\n    x ∈ Nat\n    x = 2\n")
    ra = _evident('query', str(f), 'A', '--json')
    rb = _evident('query', str(f), 'B', '--json')
    assert json.loads(ra.stdout)['x'] == 1   # SAT: raw bindings
    assert json.loads(rb.stdout)['x'] == 2
