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
    Sample a single model of a schema and return a normalised result.

    Returns: {"satisfied": bool, "bindings": {name: value, ...}}

    Backed by `evident sample <file> <schema> -n 1 --json`, which prints
    a JSON array of models (the `query` subcommand was removed):
      SAT:   stdout = `[{binding: value, ...}]`  → first model = bindings
      UNSAT: stdout = `[]` (or empty / load error) → no model
    A single-model sample is exactly a SAT/UNSAT decision plus one
    witnessing assignment, which is what the old `query` returned.
    """
    with tempfile.NamedTemporaryFile(suffix='.ev', mode='w',
                                    delete=False, dir='/tmp') as f:
        f.write(source)
        tmp = f.name
    try:
        args = ['sample', tmp, schema, '-n', '1', '--json']
        if given:
            # Pass all given values in one --given flag (space-separated).
            args += ['--given'] + [f'{k}={v}' for k, v in given.items()]
        result = _evident(*args, timeout=timeout)
        raw = result.stdout.strip()
        try:
            models = json.loads(raw) if raw else []
        except json.JSONDecodeError:
            models = []
        if isinstance(models, list) and models:
            return {'satisfied': True, 'bindings': models[0] or {}}
        return {'satisfied': False, 'bindings': {}}
    except Exception as e:
        return {'satisfied': False, 'bindings': {}, '_error': str(e)}
    finally:
        os.unlink(tmp)


def check(source: str, timeout: int = 30) -> dict[str, bool]:
    """
    Sat-check every schema in the file; return {schema_name: satisfied}.

    Backed by `evident sample <file> --all --json` (the `check`
    subcommand was removed — `sample --all` subsumes it). `--all --json`
    emits a single JSON object `{"<schema>": <bool>, ...}`.
    """
    with tempfile.NamedTemporaryFile(suffix='.ev', mode='w',
                                    delete=False, dir='/tmp') as f:
        f.write(source)
        tmp = f.name
    try:
        result = _evident('sample', tmp, '--all', '--json', timeout=timeout)
        raw = result.stdout.strip()
        try:
            parsed = json.loads(raw) if raw else {}
        except json.JSONDecodeError:
            return {}
        return {k: bool(v) for k, v in parsed.items()} if isinstance(parsed, dict) else {}
    finally:
        os.unlink(tmp)


def parse_errors(source: str, timeout: int = 10) -> list[dict]:
    """
    Load a (possibly malformed) source and return parse/load errors.
    Backed by `evident sample <file> --all --json`; a non-zero exit
    means the file failed to load, surfacing the error on stderr.
    """
    with tempfile.NamedTemporaryFile(suffix='.ev', mode='w',
                                    delete=False, dir='/tmp') as f:
        f.write(source)
        tmp = f.name
    try:
        result = _evident('sample', tmp, '--all', '--json', timeout=timeout)
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
