"""
Shared utilities for Evident conformance tests.

Black-box tests against the `evident` CLI. They specify what the language
implementation must do, regardless of how it's implemented.
"""

import json
import os
import shlex
import subprocess
import tempfile
from pathlib import Path
from typing import Any

PROJECT_ROOT = Path(__file__).parent.parent.parent


# Default to the Rust binary built under runtime/target/release.
# Override via EVIDENT_CMD env var if you want to test a different
# binary or build profile.
_DEFAULT_CMD = str(PROJECT_ROOT / 'runtime' / 'target' / 'release' / 'evident')
EVIDENT_CMD = shlex.split(os.environ.get('EVIDENT_CMD', _DEFAULT_CMD))


# (Removed: KNOWN_FAILING dict + pytest_collection_modifyitems hook
# that applied xfail markers from it. Conformance tests don't carry
# xfail/skip sediment — see lints/rules/AP-004. If a test fails:
# fix it, delete it, or file the runtime gap in
# examples/COUNTEREXAMPLES.md and delete it.)


# ---------------------------------------------------------------------------
# CLI runners
# ---------------------------------------------------------------------------

def _evident(*args: str, stdin: str | None = None, timeout: int = 30) -> subprocess.CompletedProcess:
    """Run the evident CLI with the given arguments."""
    return subprocess.run(
        [*EVIDENT_CMD, *args],
        input=stdin,
        capture_output=True,
        text=True,
        timeout=timeout,
        cwd=PROJECT_ROOT,
    )


def query(source: str, schema: str, given: dict[str, Any] | None = None,
          timeout: int = 30) -> dict:
    """
    Query a schema and return a normalised result dict.

    Returns: {"satisfied": bool, "bindings": {name: value, ...}}

    The evident CLI uses two different JSON formats:
      SAT:   exit 0, stdout = {binding: value, ...}  (just the bindings)
      UNSAT: exit 1, stdout = {"satisfied": false}
    This helper normalises both into a single consistent structure.
    """
    with tempfile.NamedTemporaryFile(suffix='.ev', mode='w',
                                    delete=False, dir='/tmp') as f:
        f.write(source)
        tmp = f.name
    try:
        args = ['query', tmp, schema, '--json']
        if given:
            # Pass all given values in one --given flag (space-separated)
            # Multiple --given flags only keeps the last one (argparse limitation)
            args += ['--given'] + [f'{k}={v}' for k, v in given.items()]
        result = _evident(*args, timeout=timeout)
        raw = result.stdout.strip()
        if result.returncode == 0 and raw:
            parsed = json.loads(raw)
            # Two SAT shapes are accepted so the same suite runs against
            # either the Python reference or the Rust port:
            #   Python: `{<binding>: <value>, ...}` (just the bindings)
            #   Rust:   `{"satisfied": true, "bindings": {<binding>: <value>}}`
            if isinstance(parsed, dict) and 'satisfied' in parsed and 'bindings' in parsed:
                return {'satisfied': bool(parsed['satisfied']),
                        'bindings': parsed.get('bindings') or {}}
            return {'satisfied': True, 'bindings': parsed}
        elif raw:
            parsed = json.loads(raw)
            if 'satisfied' in parsed:
                return {'satisfied': parsed['satisfied'],
                        'bindings': parsed.get('bindings') or {}}
        return {'satisfied': False, 'bindings': {}}
    except (json.JSONDecodeError, Exception) as e:
        return {'satisfied': False, 'bindings': {}, '_error': str(e)}
    finally:
        os.unlink(tmp)


def check(source: str, timeout: int = 30) -> dict[str, bool]:
    """
    Run evident check and return {schema_name: satisfied} for all schemas.
    Parses the text output (✓ / ✗ prefix lines).
    """
    with tempfile.NamedTemporaryFile(suffix='.ev', mode='w',
                                    delete=False, dir='/tmp') as f:
        f.write(source)
        tmp = f.name
    try:
        result = _evident('check', tmp, timeout=timeout)
        results = {}
        for line in (result.stdout + result.stderr).splitlines():
            line = line.strip()
            if line.startswith('✓') or line.startswith('✗'):
                satisfied = line.startswith('✓')
                name = line[1:].strip()
                results[name] = satisfied
        return results
    finally:
        os.unlink(tmp)


def execute(program_path: str, stdin_text: str = '',
            timeout: int = 30) -> str:
    """
    Run a program via `evident execute` and return its stdout.
    """
    result = _evident('execute', program_path, stdin=stdin_text, timeout=timeout)
    assert result.returncode == 0, (
        f"evident execute failed:\nstderr: {result.stderr}"
    )
    return result.stdout


def parse_errors(source: str, timeout: int = 10) -> list[dict]:
    """
    Run evident check and return parse errors (expecting failure).
    Returns list of error dicts or empty list if no errors.
    """
    with tempfile.NamedTemporaryFile(suffix='.ev', mode='w',
                                    delete=False, dir='/tmp') as f:
        f.write(source)
        tmp = f.name
    try:
        result = _evident('check', tmp, '--json', timeout=timeout)
        if result.returncode != 0:
            # Try to extract structured errors from stderr
            return [{'message': result.stderr.strip()}]
        return []
    finally:
        os.unlink(tmp)


# ---------------------------------------------------------------------------
# Assertion helpers
# ---------------------------------------------------------------------------

def assert_sat(result: dict) -> dict:
    """Assert the query was satisfied and return bindings."""
    assert result['satisfied'], f"Expected SAT but got UNSAT. Result: {result}"
    return result['bindings']


def assert_unsat(result: dict) -> None:
    """Assert the query was not satisfied."""
    assert not result['satisfied'], f"Expected UNSAT but got SAT. Bindings: {result.get('bindings')}"


def assert_binding(bindings: dict, name: str, value: Any) -> None:
    """Assert a specific binding has an exact value."""
    assert name in bindings, f"Binding '{name}' not found. Available: {list(bindings)}"
    assert bindings[name] == value, (
        f"Binding '{name}': expected {value!r}, got {bindings[name]!r}"
    )


def assert_binding_satisfies(bindings: dict, name: str, predicate) -> None:
    """Assert a binding satisfies a predicate."""
    assert name in bindings, f"Binding '{name}' not found."
    val = bindings[name]
    assert predicate(val), f"Binding '{name}' = {val!r} does not satisfy predicate"
