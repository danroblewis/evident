"""
Program execution conformance tests.

Tests every executable program in programs/ through the CLI.
Black-box: only stdin/stdout, no internal imports.
"""

import pytest
from pathlib import Path
from .conftest import execute, _evident

PROJECT_ROOT = Path(__file__).parent.parent.parent
PROGRAMS = PROJECT_ROOT / 'programs'


def nl(program: str, text: str) -> str:
    return execute(str(PROGRAMS / program), stdin_text=text)


# ---------------------------------------------------------------------------
# ev-nl.ev — character-by-character streaming nl
# ---------------------------------------------------------------------------

def test_ev_nl_basic():
    assert nl('ev-nl.ev', "hello\nworld\nfoo\n") == "1\thello\n2\tworld\n3\tfoo\n"

def test_ev_nl_empty():
    assert nl('ev-nl.ev', "") == ""

def test_ev_nl_no_trailing_newline():
    assert nl('ev-nl.ev', "only") == "1\tonly\n"

def test_ev_nl_blank_lines():
    assert nl('ev-nl.ev', "\n\n\n") == "1\t\n2\t\n3\t\n"

def test_ev_nl_single_line():
    assert nl('ev-nl.ev', "hello\n") == "1\thello\n"


# ---------------------------------------------------------------------------
# ev-nl-v2.ev — LineReader abstraction
# ---------------------------------------------------------------------------

def test_ev_nl_v2_basic():
    assert nl('ev-nl-v2.ev', "hello\nworld\nfoo\n") == "1\thello\n2\tworld\n3\tfoo\n"

def test_ev_nl_v2_empty():
    assert nl('ev-nl-v2.ev', "") == ""

def test_ev_nl_v2_no_trailing_newline():
    assert nl('ev-nl-v2.ev', "only") == "1\tonly\n"


# ---------------------------------------------------------------------------
# ev-nl-v3.ev — NumberedLine bidirectional relation
# ---------------------------------------------------------------------------

def test_ev_nl_v3_basic():
    assert nl('ev-nl-v3.ev', "hello\nworld\nfoo\n") == "1\thello\n2\tworld\n3\tfoo\n"

def test_ev_nl_v3_tabs_in_content():
    result = nl('ev-nl-v3.ev', "a\tb\nc\n")
    assert result == "1\ta\tb\n2\tc\n"

def test_ev_nl_v3_no_trailing_newline():
    assert nl('ev-nl-v3.ev', "last") == "1\tlast\n"


# ---------------------------------------------------------------------------
# ev-un-nl.ev — strips line numbers (reverse of ev-nl-v3)
# ---------------------------------------------------------------------------

def test_ev_un_nl_basic():
    numbered = nl('ev-nl-v3.ev', "hello\nworld\nfoo\n")
    restored = nl('ev-un-nl.ev', numbered)
    assert restored == "hello\nworld\nfoo\n"

def test_ev_un_nl_round_trip_tabs():
    text = "a\tb\nc\n"
    assert nl('ev-un-nl.ev', nl('ev-nl-v3.ev', text)) == text

def test_ev_un_nl_single():
    assert nl('ev-un-nl.ev', "1\thello\n") == "hello\n"


# ---------------------------------------------------------------------------
# number-lines.ev — streaming NumberedDocument
# ---------------------------------------------------------------------------

def test_number_lines_basic():
    assert nl('number-lines.ev', "hello\nworld\nfoo\n") == "1\thello\n2\tworld\n3\tfoo\n"

def test_number_lines_empty():
    assert nl('number-lines.ev', "") == ""

def test_number_lines_no_trailing_newline():
    assert nl('number-lines.ev', "only") == "1\tonly\n"


# ---------------------------------------------------------------------------
# strip-numbers.ev — reverse of number-lines
# ---------------------------------------------------------------------------

def test_strip_numbers_basic():
    numbered = nl('number-lines.ev', "hello\nworld\nfoo\n")
    assert nl('strip-numbers.ev', numbered) == "hello\nworld\nfoo\n"

def test_strip_numbers_round_trip():
    text = "first\nsecond\nthird\n"
    assert nl('strip-numbers.ev', nl('number-lines.ev', text)) == text


# ---------------------------------------------------------------------------
# nl-batch.ev — batch NumberedDocument (StdinLines → StdoutLines)
# ---------------------------------------------------------------------------

def test_nl_batch_basic():
    assert nl('nl-batch.ev', "hello\nworld\nfoo\n") == "1\thello\n2\tworld\n3\tfoo\n"

def test_nl_batch_single():
    assert nl('nl-batch.ev', "only\n") == "1\tonly\n"

def test_nl_batch_two_lines():
    assert nl('nl-batch.ev', "a\nb\n") == "1\ta\n2\tb\n"


# ---------------------------------------------------------------------------
# un-nl-batch.ev — batch reverse
# ---------------------------------------------------------------------------

def test_un_nl_batch_basic():
    numbered = nl('nl-batch.ev', "hello\nworld\nfoo\n")
    assert nl('un-nl-batch.ev', numbered) == "hello\nworld\nfoo\n"

def test_un_nl_batch_round_trip():
    text = "alpha\nbeta\ngamma\n"
    assert nl('un-nl-batch.ev', nl('nl-batch.ev', text)) == text


# ---------------------------------------------------------------------------
# Cross-program equivalence: all nl variants produce identical output
# ---------------------------------------------------------------------------

@pytest.mark.parametrize("program", [
    'ev-nl.ev', 'ev-nl-v2.ev', 'ev-nl-v3.ev', 'number-lines.ev', 'nl-batch.ev'
])
def test_all_nl_variants_equivalent(program):
    text = "line one\nline two\nline three\n"
    expected = "1\tline one\n2\tline two\n3\tline three\n"
    assert nl(program, text) == expected, f"{program} produced unexpected output"
