"""Tests for the constraint automaton executor (ev-nl and similar)."""

import io
import sys
import pathlib

import pytest

sys.path.insert(0, str(pathlib.Path(__file__).parent.parent.parent))


def _executor_with_source(source: str):
    from runtime.src.executor import EvidentExecutor
    ex = EvidentExecutor()
    ex.load_source(source, load_stdlib=False)
    return ex


def _run(source: str, stdin_text: str) -> str:
    ex = _executor_with_source(source)
    inp = io.StringIO(stdin_text)
    out = io.StringIO()
    ex.run(input_stream=inp, output_stream=out)
    return out.getvalue()


# ---------------------------------------------------------------------------
# Minimal inline program (no NlState, no io trait passthrough needed)
# ---------------------------------------------------------------------------

MINIMAL_IO = """\
-- Minimal I/O schemas for tests (stdlib normally provides these via import)
schema Descriptor
    fd       ∈ Nat
    open     ∈ Bool
    blocking ∈ Bool

schema Stdin
    ..Descriptor
    available ∈ Nat
    eof       ∈ Bool
    char      ∈ String
    fd = 0
    blocking = true

schema Stdout
    ..Descriptor
    out         ∈ String
    send_buffer ∈ Nat
    buffer_size ∈ Nat
    buffered    ∈ Nat
    flushed     ∈ Bool
    fd = 1
    open = true
"""

NL_SOURCE = MINIMAL_IO + """\
schema NlState
    n       ∈ Nat
    partial ∈ String

schema main
    src        ∈ Stdin
    dst        ∈ Stdout
    state      ∈ NlState
    state_next ∈ NlState
    line_num   ∈ Nat

    line_num = state.n + 1

    (src.char ≠ "\\n" ∧ src.eof = false) ⇒ dst.out            = ""
    (src.char ≠ "\\n" ∧ src.eof = false) ⇒ state_next.n       = state.n
    (src.char ≠ "\\n" ∧ src.eof = false) ⇒ state_next.partial = state.partial ++ src.char

    src.char = "\\n" ⇒ dst.out            = int_to_str line_num ++ "\\t" ++ state.partial ++ "\\n"
    src.char = "\\n" ⇒ state_next.n       = state.n + 1
    src.char = "\\n" ⇒ state_next.partial = ""

    (src.eof = true ∧ state.partial ≠ "") ⇒ dst.out = int_to_str line_num ++ "\\t" ++ state.partial ++ "\\n"
    (src.eof = true ∧ state.partial = "")  ⇒ dst.out = ""
    src.eof = true ⇒ state_next.n       = state.n
    src.eof = true ⇒ state_next.partial  = state.partial
"""


class TestEvNl:
    def test_single_line(self):
        result = _run(NL_SOURCE, "hello\n")
        assert result == "1\thello\n"

    def test_three_lines(self):
        result = _run(NL_SOURCE, "foo\nbar\nbaz\n")
        assert result == "1\tfoo\n2\tbar\n3\tbaz\n"

    def test_empty_lines(self):
        result = _run(NL_SOURCE, "\n\n\n")
        assert result == "1\t\n2\t\n3\t\n"

    def test_no_trailing_newline(self):
        # Last line has no \n — should be flushed on EOF
        result = _run(NL_SOURCE, "hello\nworld")
        assert result == "1\thello\n2\tworld\n"

    def test_empty_input(self):
        result = _run(NL_SOURCE, "")
        assert result == ""

    def test_single_line_no_newline(self):
        result = _run(NL_SOURCE, "only")
        assert result == "1\tonly\n"

    def test_numbering_is_sequential(self):
        lines = ["line" + str(i) for i in range(10)]
        stdin_text = "\n".join(lines) + "\n"
        result = _run(NL_SOURCE, stdin_text)
        for i, line in enumerate(lines, 1):
            assert f"{i}\t{line}" in result

    def test_output_line_count_matches_input(self):
        stdin_text = "a\nb\nc\nd\ne\n"
        result = _run(NL_SOURCE, stdin_text)
        assert result.count('\n') == 5


class TestExecutorInspection:
    def test_inspects_port_vars(self):
        from runtime.src.executor import EvidentExecutor
        ex = _executor_with_source(NL_SOURCE)
        inp, out, state = ex._inspect_main()
        assert 'src' in inp
        assert 'dst' in out
        assert 'state' in state

    def test_state_pair_detected(self):
        from runtime.src.executor import EvidentExecutor
        ex = _executor_with_source(NL_SOURCE)
        _, _, state = ex._inspect_main()
        assert 'state' in state
        next_var, type_name = state['state']
        assert next_var == 'state_next'
        assert type_name == 'NlState'

    def test_initial_state_defaults(self):
        from runtime.src.executor import EvidentExecutor
        ex = _executor_with_source(NL_SOURCE)
        init = ex._initial_state('NlState')
        assert init.get('n') == 0
        assert init.get('partial') == ''
