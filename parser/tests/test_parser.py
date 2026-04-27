import json
import pytest
from pathlib import Path

VALID_DIR = Path(__file__).parent / "fixtures" / "valid"
INVALID_DIR = Path(__file__).parent / "fixtures" / "invalid"

# from evident.parser import parse   # uncomment when implemented


def valid_fixtures():
    return sorted(VALID_DIR.glob("*.ev"))

def invalid_fixtures():
    return sorted(INVALID_DIR.glob("*.ev"))


@pytest.mark.parametrize("path", valid_fixtures(), ids=lambda p: p.name)
def test_valid_parses(path):
    source = path.read_text()
    # ast = parse(source)
    # assert ast is not None
    # assert ast.__class__.__name__ == "Program"
    assert len(source) > 0   # placeholder


@pytest.mark.parametrize("path", valid_fixtures(), ids=lambda p: p.name)
def test_valid_ast_matches_expected(path):
    expected_path = path.with_suffix(".expected.json")
    if not expected_path.exists():
        pytest.skip("no expected AST for this fixture yet")

    source = path.read_text()
    expected = json.loads(expected_path.read_text())

    # ast = parse(source)
    # assert ast_to_dict(ast) == expected

    assert expected["type"] == "Program"   # placeholder


@pytest.mark.parametrize("path", invalid_fixtures(), ids=lambda p: p.name)
def test_invalid_raises(path):
    source = path.read_text()
    # with pytest.raises(ParseError):
    #     parse(source)
    assert len(source) > 0   # placeholder
