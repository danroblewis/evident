"""Stdin/Stdout/Stderr stream plugins — character-at-a-time streaming I/O."""

from __future__ import annotations

import sys
from typing import Any

from ..plugin import Plugin


class StdinPlugin(Plugin):
    """Read one character per step from a text stream (default: sys.stdin).

    Halts when the stream returns EOF: emits one final "eof=True" step so the
    program can flush partial output, then signals halt.
    """

    handles_types = {'Stdin', 'CharInput'}

    def __init__(self, stream=None):
        super().__init__()
        self.stream = stream if stream is not None else sys.stdin
        self._eof = False
        self._eof_emitted = False

    def before_step(self, _state) -> dict[str, Any] | None:
        if self._eof_emitted:
            return None  # halt — already gave the program one EOF step

        char = self.stream.read(1)
        if char == '':
            self._eof = True
            self._eof_emitted = True

        given: dict[str, Any] = {}
        for var in self.matched_vars:
            given.update({
                f'{var}.fd':        0,
                f'{var}.open':      not self._eof,
                f'{var}.blocking':  True,
                f'{var}.available': 0 if self._eof else 1,
                f'{var}.eof':       self._eof,
                f'{var}.char':      char,
            })
        return given


class StdoutPlugin(Plugin):
    """Write each step's `var.out` binding to a text stream.

    Handles `Stdout`, `Stderr`, and the more general `CharOutput`. Multiple
    output variables are concatenated in declaration order (executor's dict
    preserves insertion order, which mirrors source order).
    """

    handles_types = {'Stdout', 'Stderr', 'CharOutput'}

    def __init__(self, stream=None, err_stream=None):
        super().__init__()
        self.stream     = stream     if stream     is not None else sys.stdout
        self.err_stream = err_stream if err_stream is not None else sys.stderr

    def before_step(self, _state) -> dict[str, Any]:
        # Constrain the structural fields so the solver doesn't have to choose.
        given: dict[str, Any] = {}
        for var in self.matched_vars:
            given.update({
                f'{var}.fd':          1,
                f'{var}.open':        True,
                f'{var}.blocking':    True,
                f'{var}.send_buffer': 0,
                f'{var}.buffer_size': 8192,
                f'{var}.buffered':    0,
                f'{var}.flushed':     True,
            })
        return given

    def after_step(self, bindings) -> bool:
        for var, type_name in self.matched_vars.items():
            out = bindings.get(f'{var}.out', '')
            if not out:
                continue
            target = self.err_stream if type_name == 'Stderr' else self.stream
            target.write(str(out))
            target.flush()
        return True
