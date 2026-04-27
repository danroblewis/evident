import json
import dataclasses
import pytest
from pathlib import Path

import sys
sys.path.insert(0, str(Path(__file__).parent.parent.parent))

from parser.src.parser import parse
from parser.src.ast import Program

VALID_DIR = Path(__file__).parent / "fixtures" / "valid"
INVALID_DIR = Path(__file__).parent / "fixtures" / "invalid"


def valid_fixtures():
    return sorted(VALID_DIR.glob("*.ev"))

def invalid_fixtures():
    return sorted(INVALID_DIR.glob("*.ev"))


def ast_to_dict(node):
    """Convert AST dataclass nodes to JSON-compatible dicts."""
    if node is None:
        return None
    if isinstance(node, bool):
        return node
    if isinstance(node, (int, float, str)):
        return node
    if isinstance(node, list):
        return [ast_to_dict(item) for item in node]
    if dataclasses.is_dataclass(node):
        d = {"type": type(node).__name__}
        for f in dataclasses.fields(node):
            val = getattr(node, f.name)
            # Skip fields that are at their default value (None or False)
            if val is None and f.default is None:
                continue
            if val is False and f.default is False:
                continue
            d[f.name] = ast_to_dict(val)
        return d
    return str(node)


@pytest.mark.parametrize("path", valid_fixtures(), ids=lambda p: p.name)
def test_valid_parses(path):
    source = path.read_text()
    ast = parse(source)
    assert ast is not None
    assert isinstance(ast, Program)


@pytest.mark.parametrize("path", valid_fixtures(), ids=lambda p: p.name)
def test_valid_ast_matches_expected(path):
    expected_path = path.with_suffix(".expected.json")
    if not expected_path.exists():
        pytest.skip("no expected AST for this fixture yet")

    source = path.read_text()
    expected = json.loads(expected_path.read_text())

    ast = parse(source)
    assert ast_to_dict(ast) == expected


@pytest.mark.parametrize("path", invalid_fixtures(), ids=lambda p: p.name)
def test_invalid_raises(path):
    source = path.read_text()
    with pytest.raises(Exception):
        parse(source)
