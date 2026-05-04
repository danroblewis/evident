"""Batch I/O plugins — entire input given at once, one solve, all output written.

These plugins are one-shot: their first `before_step` provides all input as
given values; their second `before_step` returns None to halt the loop.
"""

from __future__ import annotations

import sys
from typing import Any

from ..plugin import Plugin


class BatchInputPlugin(Plugin):
    """Read the whole input stream once, then signal halt."""

    handles_types = {'StdinLines', 'StdinAll', 'StdinChunks'}

    def __init__(self, stream=None):
        super().__init__()
        self.stream = stream if stream is not None else sys.stdin
        self._consumed = False

    def before_step(self, _state) -> dict[str, Any] | None:
        if self._consumed:
            return None  # one-shot: halt after the single solve
        self._consumed = True

        given: dict[str, Any] = {}
        for var, type_name in self.matched_vars.items():
            if type_name == 'StdinLines':
                given[f'{var}.lines']   = [ln.rstrip('\n') for ln in self.stream]
            elif type_name == 'StdinAll':
                given[f'{var}.content'] = self.stream.read()
            elif type_name == 'StdinChunks':
                given[f'{var}.chunks']  = [ln.rstrip('\n') for ln in self.stream]
        return given


class BatchOutputPlugin(Plugin):
    """Write the whole output once after the single solve."""

    handles_types = {'StdoutLines', 'StdoutAll'}

    def __init__(self, stream=None):
        super().__init__()
        self.stream = stream if stream is not None else sys.stdout

    def after_step(self, bindings) -> bool:
        for var, type_name in self.matched_vars.items():
            if type_name == 'StdoutLines':
                i = 0
                while True:
                    key = f'{var}.lines.{i}'
                    if key not in bindings:
                        break
                    val = bindings[key]
                    if val is not None:
                        self.stream.write(str(val) + '\n')
                    i += 1
            elif type_name == 'StdoutAll':
                val = bindings.get(f'{var}.content', '')
                if val:
                    self.stream.write(str(val))
        self.stream.flush()
        return True
